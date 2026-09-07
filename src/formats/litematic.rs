use crate::block_entity::BlockEntity;
use crate::entity::Entity;
use crate::formats::error::Result;
use crate::region::Region;
use crate::{BlockState, UniversalSchematic};
#[cfg(test)]
use flate2::read::GzDecoder;
#[cfg(test)]
use quartz_nbt::io::Flavor;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn is_litematic(data: &[u8]) -> bool {
    let root = match crate::formats::limits::parse_gzip_nbt(
        data,
        &crate::formats::limits::DecodeLimits::default(),
    ) {
        Ok(result) => result,
        Err(_) => return false,
    };

    // Check for required fields as per the Litematic format
    root.get::<_, i32>("Version").is_ok()
        && root.get::<_, &NbtCompound>("Metadata").is_ok()
        && root.get::<_, &NbtCompound>("Regions").is_ok()
}
/// Default compression level for litematic serialization.
/// Level 3 balances speed (~2x faster than L6) with size (~15% larger than L6).
const DEFAULT_COMPRESSION: flate2::Compression = flate2::Compression::new(3);

pub fn to_litematic(schematic: &UniversalSchematic) -> Result<Vec<u8>> {
    to_litematic_with_compression(schematic, DEFAULT_COMPRESSION)
}

// Minecraft data-version breakpoints that move the .litematic schematic Version
// (see LITEMATIC_FORMAT.md). The format is otherwise stable across these.
const DATA_VERSION_1_13_2: i32 = 1631; // Flattening → schematic Version 5
const DATA_VERSION_1_18: i32 = 2860; // negative-Y height → Version 6
const DATA_VERSION_1_19_2: i32 = 3120; // SubVersion=1 first written (within v6)
const DATA_VERSION_1_20_5: i32 = 3837; // item Components → Version 7

// The root-level `NucleationTest` schema version is shared with every other
// format whose writer preserves the tag — see `formats::NUCLEATION_TEST_FORMAT`.
use super::NUCLEATION_TEST_FORMAT;
/// Default target data version when a schematic carries none (latest canonical).
const DEFAULT_TARGET_DATA_VERSION: i32 = 4790; // 26.1.2

/// The `.litematic` `Version` (and optional `SubVersion`) that Litematica writes
/// for a given Minecraft data version. v4 (1.12) < 1631 ≤ v5 (1.13–1.17) < 2860 ≤
/// v6 (1.18–1.20.4) < 3837 ≤ v7 (1.20.5+). SubVersion=1 is cosmetic/write-only and
/// first appears at 1.19.2 (within v6) and at v7.
pub fn schematic_version_for_data_version(data_version: i32) -> (i32, Option<i32>) {
    if data_version < DATA_VERSION_1_13_2 {
        (4, None)
    } else if data_version < DATA_VERSION_1_18 {
        (5, None)
    } else if data_version < DATA_VERSION_1_20_5 {
        (
            6,
            if data_version >= DATA_VERSION_1_19_2 {
                Some(1)
            } else {
                None
            },
        )
    } else {
        (7, Some(1))
    }
}

/// Serialize a `.litematic` targeting a specific Minecraft data version: converts
/// a COPY of the schematic's block/item content to `target_data_version` (forward
/// flatten / reverse un-flatten + component squash via the DataConverter) and
/// writes the matching schematic Version header. Returns the bytes plus a
/// `LossReport` describing anything the down-conversion couldn't preserve (so it
/// can be surfaced to the user). The input schematic is left unmodified.
pub fn to_litematic_for_data_version(
    schematic: &UniversalSchematic,
    target_data_version: i32,
) -> Result<(Vec<u8>, crate::dataconverter::LossReport)> {
    let mut converted = schematic.clone();
    let report = converted.convert_to_data_version(target_data_version);
    let bytes = to_litematic(&converted)?;
    Ok((bytes, report))
}

pub fn to_litematic_with_compression(
    schematic: &UniversalSchematic,
    compression: flate2::Compression,
) -> Result<Vec<u8>> {
    let mut root = NbtCompound::new();

    // Derive the schematic Version from the target Minecraft data version so the
    // header matches the block/item content (pair with convert_to_data_version to
    // also downgrade the content). Falls back to the latest canonical version.
    let data_version = schematic
        .metadata
        .mc_version
        .unwrap_or(DEFAULT_TARGET_DATA_VERSION);
    let (version, sub_version) = schematic_version_for_data_version(data_version);

    root.insert("Version", NbtTag::Int(version));
    if let Some(sub) = sub_version {
        root.insert("SubVersion", NbtTag::Int(sub));
    }
    root.insert("MinecraftDataVersion", NbtTag::Int(data_version));

    // Add Metadata
    let metadata = create_metadata(schematic, version);
    root.insert("Metadata", NbtTag::Compound(metadata));

    // Add Regions
    let regions = create_regions(schematic, version);
    root.insert("Regions", NbtTag::Compound(regions));

    // A test the build carries with it, at the *root* beside `Metadata`.
    //
    // Root-level and not inside `Metadata` for two reasons: Litematica reads
    // `Metadata` field by field and ignores unknown root tags, so this survives
    // a trip through the mod untouched; and `create_metadata` above rebuilds
    // `Metadata` from scratch on every save, which would silently drop it.
    // Preservation is the point — a build edited in-game and re-saved must not
    // lose its test — so this is written from the schematic on every save and
    // read back in `parse_metadata`, and `litematic_preserves_an_embedded_test_across_a_resave`
    // pins it.
    if let Some(spec) = &schematic.metadata.embedded_test {
        let mut test = NbtCompound::new();
        test.insert("Format", NbtTag::Int(NUCLEATION_TEST_FORMAT));
        test.insert("Spec", NbtTag::String(spec.clone()));
        root.insert("NucleationTest", NbtTag::Compound(test));
    }

    // Compress and return the NBT data
    crate::nbt::canonicalize_compound(&mut root);
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), compression);
    quartz_nbt::io::write_nbt(
        &mut encoder,
        None,
        &root,
        quartz_nbt::io::Flavor::Uncompressed,
    )?;
    Ok(encoder.finish()?)
}

pub fn from_litematic(data: &[u8]) -> Result<UniversalSchematic> {
    from_litematic_bounded(data, &crate::formats::limits::DecodeLimits::default())
}

pub fn from_litematic_bounded(
    data: &[u8],
    limits: &crate::formats::limits::DecodeLimits,
) -> Result<UniversalSchematic> {
    let root = crate::formats::limits::parse_gzip_nbt(data, limits)?;
    preflight_litematic(&root, limits)?;

    let mut schematic = UniversalSchematic::new("Unnamed".to_string());

    // Parse Metadata
    parse_metadata(&root, &mut schematic)?;

    // Parse Regions
    parse_regions(&root, &mut schematic)?;

    limits.validate_schematic(&schematic)?;
    Ok(schematic)
}

fn preflight_litematic(
    root: &NbtCompound,
    limits: &crate::formats::limits::DecodeLimits,
) -> Result<()> {
    let regions = root.get::<_, &NbtCompound>("Regions")?;
    if regions.len() > limits.max_regions {
        return Err("region limit exceeded".into());
    }
    let mut total_volume = 0usize;
    let mut total_entities = 0usize;
    let mut total_block_entities = 0usize;
    for value in regions.inner().values() {
        let NbtTag::Compound(region) = value else {
            continue;
        };
        let size = region.get::<_, &NbtCompound>("Size")?;
        let volume = limits.check_dimensions((
            i64::from(size.get::<_, i32>("x")?).abs(),
            i64::from(size.get::<_, i32>("y")?).abs(),
            i64::from(size.get::<_, i32>("z")?).abs(),
        ))?;
        total_volume = total_volume
            .checked_add(volume)
            .ok_or("total volume overflow")?;
        if total_volume > limits.max_volume {
            return Err("total volume limit exceeded".into());
        }
        let palette = region.get::<_, &NbtList>("BlockStatePalette")?;
        if palette.len() > limits.max_palette_entries {
            return Err("palette limit exceeded".into());
        }
        total_entities = total_entities.saturating_add(
            region
                .get::<_, &NbtList>("Entities")
                .map_or(0, NbtList::len),
        );
        total_block_entities = total_block_entities.saturating_add(
            region
                .get::<_, &NbtList>("TileEntities")
                .map_or(0, NbtList::len),
        );
    }
    if total_entities > limits.max_entities {
        return Err("entity limit exceeded".into());
    }
    if total_block_entities > limits.max_block_entities {
        return Err("block-entity limit exceeded".into());
    }
    Ok(())
}

fn create_metadata(schematic: &UniversalSchematic, version: i32) -> NbtCompound {
    let mut metadata = NbtCompound::new();

    metadata.insert(
        "Name",
        NbtTag::String(schematic.metadata.name.clone().unwrap_or_default()),
    );
    metadata.insert(
        "Description",
        NbtTag::String(schematic.metadata.description.clone().unwrap_or_default()),
    );
    metadata.insert(
        "Author",
        NbtTag::String(schematic.metadata.author.clone().unwrap_or_default()),
    );

    // Get current time as milliseconds since epoch, safely handling both WASM and non-WASM environments
    let now = if let Some(time) = schematic.metadata.created {
        // Use existing timestamp if available
        time as i64
    } else {
        // Generate current timestamp based on platform. On wasm32 there is no
        // system clock (SystemTime::now panics on wasm32-unknown-unknown) and the
        // Diplomat wasm module carries no JS glue to ask the host — write epoch 0
        // and let callers set metadata.created explicitly when they care.
        #[cfg(target_arch = "wasm32")]
        let current_time = 0i64;

        #[cfg(not(target_arch = "wasm32"))]
        let current_time = {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
        };

        current_time
    };

    // Use existing modified timestamp or fall back to creation time
    let modified = schematic.metadata.modified.unwrap_or(now as u64) as i64;

    metadata.insert("TimeCreated", NbtTag::Long(now));
    metadata.insert("TimeModified", NbtTag::Long(modified));

    // Use content bounds (blocks + entities + block entities) for EnclosingSize
    // so entity positions fall inside the region bbox. Litematica filters out
    // entities outside the region bbox on load.
    let merged_region = schematic.get_merged_region();
    let (width, height, length) = if let Some(content_bounds) = merged_region.get_content_bounds() {
        content_bounds.get_dimensions()
    } else {
        merged_region.get_dimensions()
    };

    let mut enclosing_size = NbtCompound::new();
    enclosing_size.insert("x", NbtTag::Int(width));
    enclosing_size.insert("y", NbtTag::Int(height));
    enclosing_size.insert("z", NbtTag::Int(length));
    metadata.insert("EnclosingSize", NbtTag::Compound(enclosing_size));

    // v4 (1.12) wrote these as TAG_Long; v5+ switched to TAG_Int.
    if version <= 4 {
        metadata.insert("TotalVolume", NbtTag::Long(schematic.total_volume() as i64));
        metadata.insert("TotalBlocks", NbtTag::Long(schematic.total_blocks() as i64));
    } else {
        metadata.insert("TotalVolume", NbtTag::Int(schematic.total_volume()));
        metadata.insert("TotalBlocks", NbtTag::Int(schematic.total_blocks()));
    }
    metadata.insert(
        "RegionCount",
        NbtTag::Int(schematic.other_regions.len() as i32 + 1),
    );

    metadata.insert("Software", NbtTag::String("UniversalSchematic".to_string()));

    // Add NucleationDefinitions if present
    if !schematic.definition_regions.is_empty() {
        if let Ok(json) = serde_json::to_string(&schematic.definition_regions) {
            metadata.insert("NucleationDefinitions", NbtTag::String(json));
        }
    }

    if let Some(provenance) = &schematic.metadata.provenance {
        if let Ok(json) = provenance.to_json() {
            metadata.insert(
                crate::metadata::SCHEMATIC_PROVENANCE_NBT_KEY,
                NbtTag::String(json),
            );
        }
    }
    if !schematic.metadata.transformation_history.is_empty() {
        if let Ok(json) = serde_json::to_string(&schematic.metadata.transformation_history) {
            metadata.insert(
                crate::metadata::TRANSFORMATION_HISTORY_NBT_KEY,
                NbtTag::String(json),
            );
        }
    }

    metadata
}
fn create_regions(schematic: &UniversalSchematic, version: i32) -> NbtCompound {
    let mut regions = NbtCompound::new();

    for (name, region) in &schematic.get_all_regions() {
        // Use compact region to avoid huge empty space
        let compact_region = region.to_compact();

        let mut region_nbt = NbtCompound::new();

        // Position
        let mut position = NbtCompound::new();
        position.insert("x", NbtTag::Int(compact_region.position.0));
        position.insert("y", NbtTag::Int(compact_region.position.1));
        position.insert("z", NbtTag::Int(compact_region.position.2));
        region_nbt.insert("Position", NbtTag::Compound(position));

        // Size
        let mut size = NbtCompound::new();
        size.insert("x", NbtTag::Int(compact_region.size.0));
        size.insert("y", NbtTag::Int(compact_region.size.1));
        size.insert("z", NbtTag::Int(compact_region.size.2));
        region_nbt.insert("Size", NbtTag::Compound(size));

        // BlockStatePalette
        // Build reordered palette with air at index 0, and index mapping in one pass
        let mut reordered_palette: Vec<&BlockState> =
            Vec::with_capacity(compact_region.palette.len() + 1);
        let mut index_mapping = vec![0usize; compact_region.palette.len()];

        // Air always at index 0
        let air_state = BlockState::new("minecraft:air".to_string());
        let has_air = compact_region
            .palette
            .iter()
            .any(|b| b.name == "minecraft:air");
        // We'll use a reference to air from the palette or a local
        if has_air {
            // Find air in original palette and put a reference at index 0
            let air_ref = compact_region
                .palette
                .iter()
                .find(|b| b.name == "minecraft:air")
                .unwrap();
            reordered_palette.push(air_ref);
        } else {
            reordered_palette.push(&air_state);
        }

        // Add non-air blocks and build mapping simultaneously
        for (orig_idx, block) in compact_region.palette.iter().enumerate() {
            if block.name == "minecraft:air" {
                index_mapping[orig_idx] = 0;
            } else {
                index_mapping[orig_idx] = reordered_palette.len();
                reordered_palette.push(block);
            }
        }

        // Create the NBT list for the reordered palette
        let palette = NbtList::from(
            reordered_palette
                .iter()
                .map(|block_state| block_state.to_nbt())
                .collect::<Vec<NbtTag>>(),
        );
        region_nbt.insert("BlockStatePalette", NbtTag::List(palette));

        // Remap block indices and create packed states
        // Use integer log2 instead of floating-point
        let bits_per_block = if reordered_palette.len() <= 1 {
            2 // minimum 2 bits per block per litematic spec
        } else {
            std::cmp::max(
                (usize::BITS - (reordered_palette.len() - 1).leading_zeros()) as usize,
                2,
            )
        };
        let block_count = compact_region.blocks.len();
        let expected_len = (block_count * bits_per_block).div_ceil(64);

        let mut packed_states = vec![0i64; expected_len];
        let mask = (1u64 << bits_per_block) - 1;

        if 64 % bits_per_block == 0 {
            // Fast path: entries never cross i64 boundaries (bits_per_block = 2, 4, 8, 16, 32)
            // Batch-pack without per-block division or branching
            let entries_per_long = 64 / bits_per_block;
            for (chunk_idx, chunk) in compact_region.blocks.chunks(entries_per_long).enumerate() {
                let mut packed: u64 = 0;
                for (i, &block_state) in chunk.iter().enumerate() {
                    packed |= (index_mapping[block_state] as u64 & mask) << (i * bits_per_block);
                }
                packed_states[chunk_idx] = packed as i64;
            }
        } else {
            // Slow path: entries may cross i64 boundaries (bits_per_block = 3, 5, 6, 7, etc.)
            for (index, &block_state) in compact_region.blocks.iter().enumerate() {
                let mapped_state = index_mapping[block_state];
                let bit_index = index * bits_per_block;
                let start_long_index = bit_index / 64;
                let end_long_index = (bit_index + bits_per_block - 1) / 64;
                let start_offset = bit_index % 64;
                let value = (mapped_state as u64) & mask;

                packed_states[start_long_index] |= (value << start_offset) as i64;
                if start_long_index != end_long_index {
                    packed_states[end_long_index] |= (value >> (64 - start_offset)) as i64;
                }
            }
        }

        region_nbt.insert("BlockStates", NbtTag::LongArray(packed_states));

        // Entities - Litematic stores entity positions relative to region Position
        let entities = NbtList::from(
            compact_region
                .entities
                .iter()
                .map(|entity| {
                    let mut entity_nbt = if let NbtTag::Compound(c) = entity.to_nbt() {
                        c
                    } else {
                        NbtCompound::new()
                    };

                    let rel_x = entity.position.0 - compact_region.position.0 as f64;
                    let rel_y = entity.position.1 - compact_region.position.1 as f64;
                    let rel_z = entity.position.2 - compact_region.position.2 as f64;

                    let pos_list = NbtList::from(vec![
                        NbtTag::Double(rel_x),
                        NbtTag::Double(rel_y),
                        NbtTag::Double(rel_z),
                    ]);
                    entity_nbt.insert("Pos", NbtTag::List(pos_list));

                    NbtTag::Compound(entity_nbt)
                })
                .collect::<Vec<NbtTag>>(),
        );
        region_nbt.insert("Entities", NbtTag::List(entities));

        // TileEntities - Litematic stores block entity coordinates relative to region Position
        let tile_entities = NbtList::from(
            compact_region
                .block_entities
                .values()
                .map(|block_entity| {
                    let mut block_entity_nbt = block_entity.to_nbt();

                    let rel_x = block_entity.position.0 - compact_region.position.0;
                    let rel_y = block_entity.position.1 - compact_region.position.1;
                    let rel_z = block_entity.position.2 - compact_region.position.2;

                    block_entity_nbt.insert("x", NbtTag::Int(rel_x));
                    block_entity_nbt.insert("y", NbtTag::Int(rel_y));
                    block_entity_nbt.insert("z", NbtTag::Int(rel_z));
                    block_entity_nbt.insert("Pos", NbtTag::IntArray(vec![rel_x, rel_y, rel_z]));

                    NbtTag::Compound(block_entity_nbt)
                })
                .collect::<Vec<NbtTag>>(),
        );
        region_nbt.insert("TileEntities", NbtTag::List(tile_entities));

        // PendingBlockTicks (since schematic v3) and PendingFluidTicks (since v5 /
        // MC 1.13 — fluids did not exist in 1.12, so a v4 file must NOT carry it).
        // Not yet round-tripped; written as empty lists.
        region_nbt.insert("PendingBlockTicks", NbtTag::List(NbtList::new()));
        if version >= 5 {
            region_nbt.insert("PendingFluidTicks", NbtTag::List(NbtList::new()));
        }

        regions.insert(name, NbtTag::Compound(region_nbt));
    }

    regions
}

fn parse_metadata(root: &NbtCompound, schematic: &mut UniversalSchematic) -> Result<()> {
    // Capture the file's Minecraft data version (root-level, written by Litematica
    // as `MinecraftDataVersion`) so importers know what to forward-convert from.
    if let Ok(dv) = root.get::<_, i32>("MinecraftDataVersion") {
        schematic.metadata.mc_version = Some(dv);
        schematic.metadata.source_data_version = Some(dv);
    }

    // The test the build carries, if any. Root-level, beside `Metadata` — see
    // the writer for why it is not inside it. An unknown `Format` is read
    // anyway: the descriptor is JSON and the runner reports what it cannot
    // parse, which beats a file that silently claims to have no test.
    if let Ok(test) = root.get::<_, &NbtCompound>("NucleationTest") {
        schematic.metadata.embedded_test = test.get::<_, &str>("Spec").ok().map(String::from);
    }

    let metadata = root.get::<_, &NbtCompound>("Metadata")?;

    schematic.metadata.name = metadata.get::<_, &str>("Name").ok().map(String::from);
    schematic.metadata.description = metadata
        .get::<_, &str>("Description")
        .ok()
        .map(String::from);
    schematic.metadata.author = metadata.get::<_, &str>("Author").ok().map(String::from);
    schematic.metadata.created = metadata.get::<_, i64>("TimeCreated").ok().map(|t| t as u64);
    schematic.metadata.modified = metadata
        .get::<_, i64>("TimeModified")
        .ok()
        .map(|t| t as u64);

    schematic.metadata.provenance = metadata
        .get::<_, &str>(crate::metadata::SCHEMATIC_PROVENANCE_NBT_KEY)
        .ok()
        .and_then(|json| crate::SchematicProvenance::from_json(json).ok());
    schematic.metadata.transformation_history = metadata
        .get::<_, &str>(crate::metadata::TRANSFORMATION_HISTORY_NBT_KEY)
        .ok()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();

    // We don't need to parse EnclosingSize, TotalVolume, TotalBlocks as they will be recalculated

    // Parse NucleationDefinitions
    if let Ok(json) = metadata.get::<_, &str>("NucleationDefinitions") {
        if let Ok(regions) = serde_json::from_str(json) {
            schematic.definition_regions = regions;
        }
    }

    Ok(())
}

fn parse_regions(root: &NbtCompound, schematic: &mut UniversalSchematic) -> Result<()> {
    let regions = root.get::<_, &NbtCompound>("Regions")?;
    let mut loop_count = 0;
    for (name, region_tag) in regions.inner() {
        //if it's the first region we want to override the default region name
        if loop_count == 0 {
            schematic.default_region_name = name.clone();
        }
        loop_count += 1;

        if let NbtTag::Compound(region_nbt) = region_tag {
            let position = region_nbt.get::<_, &NbtCompound>("Position")?;
            let size = region_nbt.get::<_, &NbtCompound>("Size")?;

            let position = (
                position.get::<_, i32>("x")?,
                position.get::<_, i32>("y")?,
                position.get::<_, i32>("z")?,
            );
            let size = (
                size.get::<_, i32>("x")?,
                size.get::<_, i32>("y")?,
                size.get::<_, i32>("z")?,
            );

            let mut region = Region::new(name.to_string(), position, size);

            // Parse BlockStatePalette
            let palette = region_nbt.get::<_, &NbtList>("BlockStatePalette")?;
            region.palette = palette
                .iter()
                .filter_map(|tag| {
                    if let NbtTag::Compound(compound) = tag {
                        BlockState::from_nbt(compound).ok()
                    } else {
                        None
                    }
                })
                .collect();

            // Parse BlockStates
            let block_states = region_nbt.get::<_, &[i64]>("BlockStates")?;
            // region.unpack_block_states(block_states);
            region.blocks = region.unpack_block_states(block_states);

            // Rebuild caches after directly setting palette and blocks
            region.rebuild_palette_index();
            region.rebuild_air_index();
            region.rebuild_non_air_count();
            region.rebuild_tight_bounds();

            // Litematic entity/tile-entity coordinates are relative to the region's
            // MIN CORNER, which is NOT `Position` when `Size` is negative (Litematica
            // stores a region as an origin + signed extent — see
            // PositionUtils.getMinCorner(regionPos, posEndRel)). The block grid already
            // uses this min corner (region.bbox.min, via BoundingBox::from_position_and_size),
            // so block entities/entities must add the SAME corner to land on their
            // blocks. Adding raw `position` shifted everything by the region height/depth
            // on negative-extent schematics and pushed every container off its block.
            let min_corner = region.get_bounding_box().min;

            // Parse Entities - positions relative to the region Position/origin
            // corner. This intentionally differs from block entities below:
            // Litematica saves entities from `entityPos - box.getPos1()` and
            // places them back with `origin + regionPos + entityPos`.
            if let Ok(entities_list) = region_nbt.get::<_, &NbtList>("Entities") {
                region.entities = entities_list
                    .iter()
                    .filter_map(|tag| {
                        if let NbtTag::Compound(compound) = tag {
                            let mut entity = Entity::from_nbt(compound).ok()?;
                            // Convert from relative to absolute position
                            entity.position.0 += position.0 as f64;
                            entity.position.1 += position.1 as f64;
                            entity.position.2 += position.2 as f64;
                            Some(entity)
                        } else {
                            None
                        }
                    })
                    .collect();
            }

            // Parse TileEntities - positions relative to the region min corner.
            if let Ok(tile_entities_list) = region_nbt.get::<_, &NbtList>("TileEntities") {
                for tag in tile_entities_list.iter() {
                    if let NbtTag::Compound(compound) = tag {
                        let mut block_entity = BlockEntity::from_nbt(compound);
                        // Convert from relative to absolute position
                        block_entity.position.0 += min_corner.0;
                        block_entity.position.1 += min_corner.1;
                        block_entity.position.2 += min_corner.2;
                        region
                            .block_entities
                            .insert(block_entity.position, block_entity);
                    }
                }
            }

            schematic.add_region(region);
        }
    }

    Ok(())
}

use crate::formats::manager::{SchematicExporter, SchematicImporter};

pub struct LitematicFormat;

impl SchematicImporter for LitematicFormat {
    fn name(&self) -> String {
        "litematic".to_string()
    }

    fn detect(&self, data: &[u8]) -> bool {
        is_litematic(data)
    }

    fn detect_bounded(&self, data: &[u8], limits: &crate::formats::limits::DecodeLimits) -> bool {
        from_litematic_bounded(data, limits).is_ok()
    }

    fn read(&self, data: &[u8]) -> Result<UniversalSchematic> {
        from_litematic(data)
    }

    fn read_bounded(
        &self,
        data: &[u8],
        limits: &crate::formats::limits::DecodeLimits,
    ) -> Result<UniversalSchematic> {
        from_litematic_bounded(data, limits)
    }
}

impl SchematicExporter for LitematicFormat {
    fn name(&self) -> String {
        "litematic".to_string()
    }

    fn extensions(&self) -> Vec<String> {
        vec!["litematic".to_string()]
    }

    fn available_versions(&self) -> Vec<String> {
        vec!["default".to_string()]
    }

    fn default_version(&self) -> String {
        "default".to_string()
    }

    fn write(&self, schematic: &UniversalSchematic, _version: Option<&str>) -> Result<Vec<u8>> {
        to_litematic(schematic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockState, UniversalSchematic};

    use std::fs::File;
    use std::io::Write;

    // Build the gzipped litematic bytes for a hand-crafted root compound.
    fn gzip_litematic(root: &NbtCompound) -> Vec<u8> {
        use flate2::{write::GzEncoder, Compression};
        let mut uncompressed = Vec::new();
        quartz_nbt::io::write_nbt(&mut uncompressed, None, root, Flavor::Uncompressed).unwrap();
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut enc, &uncompressed).unwrap();
        enc.finish().unwrap()
    }

    // Read the root NBT compound back out of produced .litematic bytes (header check).
    fn read_root(bytes: &[u8]) -> NbtCompound {
        let mut gz = GzDecoder::new(bytes);
        let (root, _) = quartz_nbt::io::read_nbt(&mut gz, Flavor::Uncompressed).unwrap();
        root
    }

    /// A litematic must carry entity state a tick simulation depends on.
    ///
    /// This is the feasibility question for using `.litematic` as the container
    /// for self-contained simulation tests: the record 3x3 piston door is held
    /// together by minecarts whose velocity is **NaN**, and by two blazes riding
    /// inside carts as `Passengers`. Both are ordinary NBT — TAG_Double is eight
    /// raw IEEE-754 bytes and a passenger is a nested compound — so a faithful
    /// writer keeps them. That is worth pinning rather than assuming, because
    /// sanitising a non-finite velocity to 0.0 turns a frozen cart into an
    /// ordinary one and quietly un-glues the machine. See
    /// `crates/mc-tick/docs/history/entity-abuse-in-record-doors.md`.
    #[test]
    fn litematic_round_trips_nan_velocities_and_passengers() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/samples/55_3x3.zip");
        let bytes = std::fs::read(&path).expect("the record-door sample must be present");
        let source = crate::formats::world::from_world_zip(&bytes).expect("the sample loads");

        // What the world itself carries, so the assertions below compare against
        // the file rather than against a number written down here.
        let count_nan = |s: &UniversalSchematic| -> usize {
            s.get_entities_as_list()
                .iter()
                .filter_map(|e| e.nbt.get("Motion"))
                .filter_map(|m| match m {
                    crate::entity::NbtValue::List(v) => Some(v),
                    _ => None,
                })
                .flatten()
                .filter(|v| matches!(v, crate::entity::NbtValue::Double(d) if d.is_nan()))
                .count()
        };
        let count_riders = |s: &UniversalSchematic| -> usize {
            s.get_entities_as_list()
                .iter()
                .filter_map(|e| e.nbt.get("Passengers"))
                .filter_map(|p| match p {
                    crate::entity::NbtValue::List(v) => Some(v.len()),
                    _ => None,
                })
                .sum()
        };

        let entities_before = source.get_entities_as_list().len();
        let nan_before = count_nan(&source);
        let riders_before = count_riders(&source);
        assert!(
            nan_before > 0,
            "the sample must contain nan carts to be worth testing"
        );
        assert!(
            riders_before > 0,
            "the sample must contain passengers to be worth testing"
        );

        let lit = to_litematic(&source).expect("the door writes as a litematic");
        let back = from_litematic(&lit).expect("and reads back");

        assert_eq!(
            back.get_entities_as_list().len(),
            entities_before,
            "every entity must survive the round trip"
        );
        assert_eq!(
            count_nan(&back),
            nan_before,
            "NaN velocities must survive — sanitising one un-glues the build"
        );
        assert_eq!(
            count_riders(&back),
            riders_before,
            "passengers ride inside their vehicle's NBT and must survive with it"
        );
    }

    /// A `.litematic` that carries its own test keeps it when it is saved again.
    ///
    /// This is the whole feature, and it is exactly the thing that was broken by
    /// construction: `create_metadata` rebuilds `Metadata` from scratch on every
    /// save, so anything stored in there is dropped on the next write. The test
    /// lives at the *root* beside `Metadata` instead, and this pins that — a
    /// build opened in Litematica, nudged, and re-saved must come back still
    /// knowing how to test itself.
    #[test]
    fn litematic_preserves_an_embedded_test_across_a_resave() {
        let spec = r#"{"name":"a door opens","checks":[{"tick":0,"expect":"initial"}]}"#;

        let mut schem = UniversalSchematic::new("carrier".into());
        schem.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));
        schem.metadata.embedded_test = Some(spec.to_string());

        // Save, load: the test comes back.
        let first = to_litematic(&schem).expect("writes");
        let reloaded = from_litematic(&first).expect("reads");
        assert_eq!(
            reloaded.metadata.embedded_test.as_deref(),
            Some(spec),
            "the embedded test must survive one round trip"
        );

        // Save *what we loaded*, load again: still there. This is the leg that
        // a Metadata-hosted spec would have failed.
        let second = to_litematic(&reloaded).expect("writes again");
        let twice = from_litematic(&second).expect("reads again");
        assert_eq!(
            twice.metadata.embedded_test.as_deref(),
            Some(spec),
            "re-saving a loaded build must not drop its test"
        );

        // And it really is at the root, where Litematica will leave it alone.
        let root = read_root(&second);
        let test = root
            .get::<_, &NbtCompound>("NucleationTest")
            .expect("NucleationTest sits at the root, beside Metadata");
        assert_eq!(
            test.get::<_, i32>("Format").unwrap(),
            NUCLEATION_TEST_FORMAT
        );
        assert!(
            root.get::<_, &NbtCompound>("Metadata")
                .unwrap()
                .get::<_, &str>("NucleationTest")
                .is_err(),
            "it must not also be inside Metadata, which is rebuilt on every save"
        );

        // A build with no test writes no tag at all, rather than an empty one.
        let plain = to_litematic(&UniversalSchematic::new("plain".into())).expect("writes");
        assert!(
            read_root(&plain)
                .get::<_, &NbtCompound>("NucleationTest")
                .is_err(),
            "a build with no test must not grow an empty NucleationTest"
        );
    }

    /// The record door's data version survives a `.litematic` round trip.
    ///
    /// Load-bearing and easy to lose: the door is DataVersion 4082, and
    /// [`crate::formats::gametest`]'s consumers pick the `Entity.load` rules
    /// from it. 4082 selects the ≤4556 semantics that *keep* a NaN velocity. A
    /// round trip that dropped or rewrote the version would load the door's nan
    /// carts as ordinary carts, and the door would silently un-glue with no
    /// error anywhere.
    #[test]
    fn litematic_round_trips_the_record_doors_data_version() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/samples/55_3x3.zip");
        let bytes = std::fs::read(&path).expect("the record-door sample must be present");
        let source = crate::formats::world::from_world_zip(&bytes).expect("the sample loads");
        assert_eq!(
            source.metadata.source_data_version,
            Some(4082),
            "the record door is 1.21.3 — if this moved, the numbers below are about a different save"
        );

        let lit = to_litematic(&source).expect("writes");
        assert_eq!(
            read_root(&lit)
                .get::<_, i32>("MinecraftDataVersion")
                .unwrap(),
            4082,
            "the file must state the version it was captured at"
        );

        let back = from_litematic(&lit).expect("reads");
        assert_eq!(
            back.metadata.source_data_version,
            Some(4082),
            "losing this loads the nan carts as ordinary carts and un-glues the door"
        );

        // And again, because a re-save is where a transient field goes missing.
        let twice = from_litematic(&to_litematic(&back).expect("writes")).expect("reads");
        assert_eq!(twice.metadata.source_data_version, Some(4082));
    }

    #[test]
    fn schematic_version_maps_data_versions_correctly() {
        assert_eq!(schematic_version_for_data_version(1343), (4, None)); // 1.12.2
        assert_eq!(schematic_version_for_data_version(1631), (5, None)); // 1.13.2
        assert_eq!(schematic_version_for_data_version(2586), (5, None)); // 1.16.5
        assert_eq!(schematic_version_for_data_version(2860), (6, None)); // 1.18
        assert_eq!(schematic_version_for_data_version(3120), (6, Some(1))); // 1.19.2
        assert_eq!(schematic_version_for_data_version(3700), (6, Some(1))); // 1.20.4
        assert_eq!(schematic_version_for_data_version(3837), (7, Some(1))); // 1.20.5
        assert_eq!(schematic_version_for_data_version(4189), (7, Some(1))); // 1.21.x
    }

    #[test]
    fn writes_version_header_matching_target_data_version() {
        let cases = [
            (1343, 4, false, false), // (data_version, expected Version, expect SubVersion, expect PendingFluidTicks)
            (1631, 5, false, true),
            (2860, 6, false, true),
            (3120, 6, true, true),
            (3837, 7, true, true),
            (4189, 7, true, true),
        ];
        for (dv, want_ver, want_sub, want_fluid) in cases {
            let mut schem = UniversalSchematic::new("v".to_string());
            schem.metadata.mc_version = Some(dv);
            schem.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));
            let bytes = to_litematic(&schem).unwrap();
            let root = read_root(&bytes);
            assert_eq!(
                root.get::<_, i32>("Version").unwrap(),
                want_ver,
                "Version for dv={}",
                dv
            );
            assert_eq!(root.get::<_, i32>("MinecraftDataVersion").unwrap(), dv);
            assert_eq!(
                root.get::<_, i32>("SubVersion").is_ok(),
                want_sub,
                "SubVersion presence for dv={}",
                dv
            );
            // PendingFluidTicks presence is gated on Version>=5.
            let regions = root.get::<_, &NbtCompound>("Regions").unwrap();
            let (_, rtag) = regions.inner().iter().next().unwrap();
            if let NbtTag::Compound(region) = rtag {
                assert_eq!(
                    region.get::<_, &NbtList>("PendingFluidTicks").is_ok(),
                    want_fluid,
                    "PendingFluidTicks presence for dv={}",
                    dv
                );
                // v4 wrote Total... only in Metadata; here just confirm block round-trips.
            }
            // The bytes re-parse and keep the block + source data version.
            let reparsed = from_litematic(&bytes).unwrap();
            assert_eq!(
                reparsed.get_block(0, 0, 0).map(|b| b.name.as_str()),
                Some("minecraft:stone")
            );
            assert_eq!(reparsed.metadata.source_data_version, Some(dv));
        }
    }

    #[test]
    fn to_litematic_for_data_version_converts_and_writes_matching_header() {
        let mut schem = UniversalSchematic::new("conv".to_string());
        schem.metadata.mc_version = Some(4189); // a modern (1.21) source
        schem.metadata.source_data_version = Some(4189);
        schem.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));

        // Target 1.12 (v4): a COPY is down-converted; the input is untouched.
        let (bytes, _loss) = to_litematic_for_data_version(&schem, 1343).unwrap();
        let root = read_root(&bytes);
        assert_eq!(root.get::<_, i32>("Version").unwrap(), 4);
        assert_eq!(root.get::<_, i32>("MinecraftDataVersion").unwrap(), 1343);
        assert_eq!(
            schem.metadata.mc_version,
            Some(4189),
            "input schematic must be unchanged"
        );
        // It re-reads and stone (unchanged by flattening) survives.
        let reparsed = from_litematic(&bytes).unwrap();
        assert_eq!(
            reparsed.get_block(0, 0, 0).map(|b| b.name.as_str()),
            Some("minecraft:stone")
        );
    }

    #[test]
    fn v4_writes_total_volume_as_long() {
        let mut schem = UniversalSchematic::new("v4".to_string());
        schem.metadata.mc_version = Some(1343);
        schem.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));
        let meta = read_root(&to_litematic(&schem).unwrap());
        let m = meta.get::<_, &NbtCompound>("Metadata").unwrap();
        assert!(matches!(
            m.get::<_, &NbtTag>("TotalVolume").unwrap(),
            NbtTag::Long(_)
        ));
        // v7 uses Int.
        let mut schem7 = UniversalSchematic::new("v7".to_string());
        schem7.metadata.mc_version = Some(4189);
        schem7.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));
        let m7root = read_root(&to_litematic(&schem7).unwrap());
        let m7 = m7root.get::<_, &NbtCompound>("Metadata").unwrap();
        assert!(matches!(
            m7.get::<_, &NbtTag>("TotalVolume").unwrap(),
            NbtTag::Int(_)
        ));
    }

    // A region with a NEGATIVE Size: Position is the origin corner, Size the signed
    // extent, so the MIN corner is below Position. Litematica stores blocks AND tile
    // entities relative to that min corner — the loader must add the min corner, not
    // Position, or every container lands off its block. Regression for the VIP /
    // negative-size schematics where item contents "didn't load".
    #[test]
    fn negative_size_region_places_block_entity_on_its_block() {
        // 1×2×1 region: Position (0,5,0), Size (1,-2,1) → min corner (0,4,0); the two
        // cells are world (0,4,0) [local idx 0] and (0,5,0) [local idx 1].
        let mut palette = NbtList::new();
        let mut air = NbtCompound::new();
        air.insert("Name", NbtTag::String("minecraft:air".to_string()));
        palette.push(NbtTag::Compound(air));
        let mut chest = NbtCompound::new();
        chest.insert("Name", NbtTag::String("minecraft:chest".to_string()));
        palette.push(NbtTag::Compound(chest));

        // blocks = [palette 1 (chest) @ idx0, palette 0 (air) @ idx1]; 2 bits each → 0b01.
        let block_states = NbtTag::LongArray(vec![1]);

        // The chest tile entity at local (0,0,0) = world (0,4,0), holding a stone.
        let mut item = NbtCompound::new();
        item.insert("id", NbtTag::String("minecraft:stone".to_string()));
        item.insert("count", NbtTag::Int(1));
        item.insert("Slot", NbtTag::Byte(0));
        let mut items = NbtList::new();
        items.push(NbtTag::Compound(item));
        let mut te = NbtCompound::new();
        te.insert("id", NbtTag::String("minecraft:chest".to_string()));
        te.insert("x", NbtTag::Int(0));
        te.insert("y", NbtTag::Int(0));
        te.insert("z", NbtTag::Int(0));
        te.insert("Items", NbtTag::List(items));
        let mut tile_entities = NbtList::new();
        tile_entities.push(NbtTag::Compound(te));

        let mut pos = NbtCompound::new();
        pos.insert("x", NbtTag::Int(0));
        pos.insert("y", NbtTag::Int(5));
        pos.insert("z", NbtTag::Int(0));
        let mut size = NbtCompound::new();
        size.insert("x", NbtTag::Int(1));
        size.insert("y", NbtTag::Int(-2));
        size.insert("z", NbtTag::Int(1));

        let mut region = NbtCompound::new();
        region.insert("Position", NbtTag::Compound(pos));
        region.insert("Size", NbtTag::Compound(size));
        region.insert("BlockStatePalette", NbtTag::List(palette));
        region.insert("BlockStates", block_states);
        region.insert("TileEntities", NbtTag::List(tile_entities));

        let mut regions = NbtCompound::new();
        regions.insert("r", NbtTag::Compound(region));
        let mut root = NbtCompound::new();
        root.insert("MinecraftDataVersion", NbtTag::Int(4189));
        root.insert("Version", NbtTag::Int(7));
        root.insert("Metadata", NbtTag::Compound(NbtCompound::new()));
        root.insert("Regions", NbtTag::Compound(regions));

        let schem = from_litematic(&gzip_litematic(&root)).expect("parse");

        // The chest block sits at world (0,4,0)...
        assert_eq!(
            schem.get_block(0, 4, 0).map(|b| b.name.as_str()),
            Some("minecraft:chest")
        );
        assert_eq!(
            schem.get_block(0, 5, 0).map(|b| b.name.as_str()),
            Some("minecraft:air")
        );
        // ...and its block entity must land on it (NOT at Position-offset (0,5,0)).
        let bes = schem.get_block_entities_as_list();
        assert_eq!(bes.len(), 1);
        assert_eq!(bes[0].id, "minecraft:chest");
        assert_eq!(
            bes[0].position,
            (0, 4, 0),
            "block entity must sit on the min-corner-relative cell"
        );
        // And its item survived.
        let items_len = bes[0]
            .nbt
            .get("Items")
            .and_then(|v| {
                if let crate::nbt::NbtValue::List(l) = v {
                    Some(l.len())
                } else {
                    None
                }
            })
            .unwrap_or(0);
        assert_eq!(items_len, 1);
    }

    // Litematica stores mobile entity Pos relative to the region Position/origin
    // corner, not the min corner used by the block array. For negative-size
    // regions those corners differ. Regression for sheep_test.litematic: a boat
    // saved at relative (-0.5, 1, -1.3125) inside Position (2,0,3), Size
    // (-3,2,-4) must load at (1.5,1,1.6875), not off-platform at
    // (-0.5,1,-1.3125).
    #[test]
    fn negative_size_region_places_entity_from_region_position() {
        let mut palette = NbtList::new();
        let mut air = NbtCompound::new();
        air.insert("Name", NbtTag::String("minecraft:air".to_string()));
        palette.push(NbtTag::Compound(air));
        let mut grass = NbtCompound::new();
        grass.insert("Name", NbtTag::String("minecraft:grass_block".to_string()));
        palette.push(NbtTag::Compound(grass));

        // 3x2x4 region, all bottom-layer cells are grass so content bounds make a platform.
        let mut block_states = 0i64;
        for index in 0..12 {
            block_states |= 1i64 << (index * 2);
        }

        let mut pos = NbtCompound::new();
        pos.insert("x", NbtTag::Int(2));
        pos.insert("y", NbtTag::Int(0));
        pos.insert("z", NbtTag::Int(3));
        let mut size = NbtCompound::new();
        size.insert("x", NbtTag::Int(-3));
        size.insert("y", NbtTag::Int(2));
        size.insert("z", NbtTag::Int(-4));

        let mut entity_pos = NbtList::new();
        entity_pos.push(NbtTag::Double(-0.5));
        entity_pos.push(NbtTag::Double(1.0));
        entity_pos.push(NbtTag::Double(-1.3125));
        let mut entity = NbtCompound::new();
        entity.insert("id", NbtTag::String("minecraft:oak_boat".to_string()));
        entity.insert("Pos", NbtTag::List(entity_pos));
        let mut entities = NbtList::new();
        entities.push(NbtTag::Compound(entity));

        let mut region = NbtCompound::new();
        region.insert("Position", NbtTag::Compound(pos));
        region.insert("Size", NbtTag::Compound(size));
        region.insert("BlockStatePalette", NbtTag::List(palette));
        region.insert("BlockStates", NbtTag::LongArray(vec![block_states]));
        region.insert("Entities", NbtTag::List(entities));

        let mut regions = NbtCompound::new();
        regions.insert("r", NbtTag::Compound(region));
        let mut root = NbtCompound::new();
        root.insert("MinecraftDataVersion", NbtTag::Int(4189));
        root.insert("Version", NbtTag::Int(7));
        root.insert("Metadata", NbtTag::Compound(NbtCompound::new()));
        root.insert("Regions", NbtTag::Compound(regions));

        let schem = from_litematic(&gzip_litematic(&root)).expect("parse");
        let entity = &schem.default_region.entities[0];
        assert!(
            (entity.position.0 - 1.5).abs() < 0.001,
            "x={}",
            entity.position.0
        );
        assert!(
            (entity.position.1 - 1.0).abs() < 0.001,
            "y={}",
            entity.position.1
        );
        assert!(
            (entity.position.2 - 1.6875).abs() < 0.001,
            "z={}",
            entity.position.2
        );
    }

    #[test]
    fn test_create_metadata() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        schematic.metadata.author = Some("Test Author".to_string());
        schematic.metadata.description = Some("Test Description".to_string());
        schematic.metadata.created = Some(1000);
        schematic.metadata.modified = Some(2000);

        let metadata = create_metadata(&schematic, 7);

        assert_eq!(metadata.get::<_, &str>("Name").unwrap(), "Test Schematic");
        assert_eq!(metadata.get::<_, &str>("Author").unwrap(), "Test Author");
        assert_eq!(
            metadata.get::<_, &str>("Description").unwrap(),
            "Test Description"
        );
        assert_eq!(metadata.get::<_, i64>("TimeCreated").unwrap(), 1000);
        assert_eq!(metadata.get::<_, i64>("TimeModified").unwrap(), 2000);
        assert!(metadata.contains_key("EnclosingSize"));
        assert!(metadata.contains_key("TotalVolume"));
        assert!(metadata.contains_key("TotalBlocks"));
        assert!(metadata.contains_key("RegionCount"));
        assert_eq!(
            metadata.get::<_, &str>("Software").unwrap(),
            "UniversalSchematic"
        );
    }

    #[test]
    fn test_create_regions() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        let mut region = Region::new("TestRegion".to_string(), (0, 0, 0), (2, 2, 2));

        let stone = BlockState::new("minecraft:stone".to_string());
        let _air = BlockState::new("minecraft:air".to_string());

        region.set_block(0, 0, 0, &stone);
        region.set_block(1, 1, 1, &stone);

        let entity = Entity::new("minecraft:creeper".to_string(), (0.5, 0.0, 0.5));
        region.add_entity(entity);

        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (0, 1, 0));
        region.add_block_entity(block_entity);

        schematic.add_region(region);

        let regions = create_regions(&schematic, 7);

        assert!(regions.contains_key("TestRegion"));
        let region_nbt = regions.get::<_, &NbtCompound>("TestRegion").unwrap();

        assert!(region_nbt.contains_key("Position"));
        assert!(region_nbt.contains_key("Size"));
        assert!(region_nbt.contains_key("BlockStatePalette"));
        assert!(region_nbt.contains_key("BlockStates"));
        assert!(region_nbt.contains_key("Entities"));
        assert!(region_nbt.contains_key("TileEntities"));
        assert!(region_nbt.contains_key("PendingBlockTicks"));
        assert!(region_nbt.contains_key("PendingFluidTicks"));
    }

    #[test]
    fn test_parse_metadata() {
        let mut root = NbtCompound::new();
        let mut metadata = NbtCompound::new();
        metadata.insert("Name", NbtTag::String("Test Schematic".to_string()));
        metadata.insert("Author", NbtTag::String("Test Author".to_string()));
        metadata.insert(
            "Description",
            NbtTag::String("Test Description".to_string()),
        );
        metadata.insert("TimeCreated", NbtTag::Long(1000));
        metadata.insert("TimeModified", NbtTag::Long(2000));
        root.insert("Metadata", NbtTag::Compound(metadata));

        let mut schematic = UniversalSchematic::new("".to_string());
        parse_metadata(&root, &mut schematic).unwrap();

        assert_eq!(schematic.metadata.name, Some("Test Schematic".to_string()));
        assert_eq!(schematic.metadata.author, Some("Test Author".to_string()));
        assert_eq!(
            schematic.metadata.description,
            Some("Test Description".to_string())
        );
        assert_eq!(schematic.metadata.created, Some(1000));
        assert_eq!(schematic.metadata.modified, Some(2000));
    }

    #[test]
    fn test_parse_regions() {
        let mut root = NbtCompound::new();
        let mut regions = NbtCompound::new();
        let mut region = NbtCompound::new();

        let mut position = NbtCompound::new();
        position.insert("x", NbtTag::Int(0));
        position.insert("y", NbtTag::Int(0));
        position.insert("z", NbtTag::Int(0));
        region.insert("Position", NbtTag::Compound(position));

        let mut size = NbtCompound::new();
        size.insert("x", NbtTag::Int(2));
        size.insert("y", NbtTag::Int(2));
        size.insert("z", NbtTag::Int(2));
        region.insert("Size", NbtTag::Compound(size));

        let palette = NbtList::from(vec![
            BlockState::new("minecraft:air".to_string()).to_nbt(),
            BlockState::new("minecraft:stone".to_string()).to_nbt(),
        ]);
        region.insert("BlockStatePalette", NbtTag::List(palette));

        // 2x2x2 region with 2 stone blocks and 6 air blocks
        region.insert("BlockStates", NbtTag::LongArray(vec![0b10000001]));

        regions.insert("TestRegion", NbtTag::Compound(region));
        root.insert("Regions", NbtTag::Compound(regions));

        println!("{:?}", root);

        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        parse_regions(&root, &mut schematic).unwrap();

        assert_eq!(schematic.default_region_name, "TestRegion");

        let parsed_region = schematic.default_region;
        assert_eq!(parsed_region.position, (0, 0, 0));
        assert_eq!(parsed_region.size, (2, 2, 2));
        assert_eq!(parsed_region.palette.len(), 2);
        assert_eq!(parsed_region.count_blocks(), 2); // 2 stone blocks
    }
    #[test]
    fn test_simple_litematic() {
        let mut schematic = UniversalSchematic::new("Simple Cube".to_string());
        schematic.metadata.created = Some(1000);
        schematic.metadata.modified = Some(2000);
        // Create a 3x3x3 cube
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    let block = match (x + y + z) % 3 {
                        0 => BlockState::new("minecraft:stone".to_string()),
                        1 => BlockState::new("minecraft:dirt".to_string()),
                        _ => BlockState::new("minecraft:oak_planks".to_string()),
                    };
                    schematic.set_block(x, y, z, &block);
                }
            }
        }

        // Set metadata
        schematic.metadata.author = Some("Test Author".to_string());
        schematic.metadata.description = Some("A simple 3x3x3 cube for testing".to_string());

        // Convert the schematic to .litematic format
        let litematic_data =
            to_litematic(&schematic).expect("Failed to convert schematic to litematic");

        // Save the .litematic file
        let mut file = File::create("simple_cube.litematic").expect("Failed to create file");
        file.write_all(&litematic_data)
            .expect("Failed to write to file");

        // Read the .litematic file back
        let _loaded_litematic_data =
            std::fs::read("simple_cube.litematic").expect("Failed to read file");

        // Clean up the generated file
        //std::fs::remove_file("simple_cube.litematic").expect("Failed to remove file");
    }

    #[test]
    fn test_litematic_roundtrip() {
        let mut original_schematic = UniversalSchematic::new("Test Schematic".to_string());
        original_schematic.metadata.created = Some(1000);
        original_schematic.metadata.modified = Some(2000);
        let mut region = Region::new("TestRegion".to_string(), (0, 0, 0), (2, 2, 2));

        let stone = BlockState::new("minecraft:stone".to_string());
        let _air = BlockState::new("minecraft:air".to_string());

        region.set_block(0, 0, 0, &stone);
        region.set_block(1, 1, 1, &stone);

        original_schematic.add_region(region);

        // Convert to Litematic
        let litematic_data = to_litematic(&original_schematic).unwrap();

        // Convert back from Litematic
        let roundtrip_schematic = from_litematic(&litematic_data).unwrap();

        // Compare original and roundtrip schematics
        assert_eq!(
            original_schematic.metadata.name,
            roundtrip_schematic.metadata.name
        );
        assert_eq!(
            original_schematic.other_regions.len(),
            roundtrip_schematic.other_regions.len()
        );

        // Compare the "TestRegion" instead of the default region
        let original_region = original_schematic.get_region("TestRegion").unwrap();
        let roundtrip_region = roundtrip_schematic.get_region("TestRegion").unwrap();

        assert_eq!(original_region.position, roundtrip_region.position);
        assert_eq!(original_region.size, roundtrip_region.size);
        assert_eq!(
            original_region.count_blocks(),
            roundtrip_region.count_blocks()
        );

        // Check if blocks are in the same positions
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    assert_eq!(
                        original_region.get_block(x, y, z),
                        roundtrip_region.get_block(x, y, z)
                    );
                }
            }
        }
    }

    /// Test that litematic export stores block entity and entity positions
    /// relative to the region Position, not as absolute coordinates.
    #[test]
    fn test_litematic_relative_positions_in_nbt() {
        let mut schematic = UniversalSchematic::new("RelativePositionTest".to_string());
        schematic.metadata.created = Some(1000);
        schematic.metadata.modified = Some(2000);

        // Place blocks and block entities at non-zero positions
        // so the compact region will have a non-zero position offset
        let stone = BlockState::new("minecraft:stone".to_string());
        schematic.set_block(10, 20, 30, &stone);
        schematic.set_block(11, 21, 31, &stone);

        // Add a block entity at absolute position (10, 20, 30)
        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (10, 20, 30));
        schematic.default_region.add_block_entity(block_entity);

        // Add an entity at absolute position (10.5, 20.0, 30.5)
        let entity = Entity::new("minecraft:creeper".to_string(), (10.5, 20.0, 30.5));
        schematic.default_region.add_entity(entity);

        // Export to litematic
        let litematic_data = to_litematic(&schematic).unwrap();

        // Parse back the raw NBT to inspect stored positions
        let mut decoder = flate2::read::GzDecoder::new(litematic_data.as_slice());
        let mut decompressed = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut decompressed).unwrap();
        let (root, _) = quartz_nbt::io::read_nbt(
            &mut std::io::Cursor::new(decompressed),
            quartz_nbt::io::Flavor::Uncompressed,
        )
        .unwrap();

        let regions = root.get::<_, &NbtCompound>("Regions").unwrap();
        // Get the first (and only) region
        let (_, region_tag) = regions.inner().iter().next().unwrap();
        let region_nbt = if let NbtTag::Compound(c) = region_tag {
            c
        } else {
            panic!("Expected compound")
        };

        // Get the region position (should be the compact region min)
        let pos_nbt = region_nbt.get::<_, &NbtCompound>("Position").unwrap();
        let region_x = pos_nbt.get::<_, i32>("x").unwrap();
        let region_y = pos_nbt.get::<_, i32>("y").unwrap();
        let region_z = pos_nbt.get::<_, i32>("z").unwrap();

        // Region position should be at the tight bounds min (10, 20, 30)
        assert_eq!(region_x, 10);
        assert_eq!(region_y, 20);
        assert_eq!(region_z, 30);

        // Check block entity positions are RELATIVE to region position
        let tile_entities = region_nbt.get::<_, &NbtList>("TileEntities").unwrap();
        assert_eq!(tile_entities.len(), 1);
        if let NbtTag::Compound(be_nbt) = &tile_entities[0] {
            // Block entity at absolute (10,20,30) with region at (10,20,30)
            // should be stored as relative (0,0,0)
            let pos = be_nbt.get::<_, &[i32]>("Pos").unwrap();
            assert_eq!(
                pos,
                &[0, 0, 0],
                "Block entity position should be relative to region, got {:?}",
                pos
            );

            assert_eq!(be_nbt.get::<_, i32>("x").unwrap(), 0);
            assert_eq!(be_nbt.get::<_, i32>("y").unwrap(), 0);
            assert_eq!(be_nbt.get::<_, i32>("z").unwrap(), 0);
        } else {
            panic!("Expected compound tag for block entity");
        }

        // Check entity positions are RELATIVE to region position
        let entities = region_nbt.get::<_, &NbtList>("Entities").unwrap();
        assert_eq!(entities.len(), 1);
        if let NbtTag::Compound(ent_nbt) = &entities[0] {
            let pos = ent_nbt.get::<_, &NbtList>("Pos").unwrap();
            // Entity at absolute (10.5, 20.0, 30.5) with region at (10, 20, 30)
            // should be stored as relative (0.5, 0.0, 0.5)
            let x = pos.get::<f64>(0).unwrap();
            let y = pos.get::<f64>(1).unwrap();
            let z = pos.get::<f64>(2).unwrap();
            assert!(
                (x - 0.5).abs() < 0.001,
                "Entity X should be 0.5 relative, got {}",
                x
            );
            assert!(
                (y - 0.0).abs() < 0.001,
                "Entity Y should be 0.0 relative, got {}",
                y
            );
            assert!(
                (z - 0.5).abs() < 0.001,
                "Entity Z should be 0.5 relative, got {}",
                z
            );
        } else {
            panic!("Expected compound tag for entity");
        }
    }

    /// Test that litematic roundtrip preserves absolute positions of block entities
    /// and entities even when the region has a non-zero position offset.
    #[test]
    fn test_litematic_roundtrip_with_offset_positions() {
        let mut schematic = UniversalSchematic::new("OffsetRoundtrip".to_string());
        schematic.metadata.created = Some(1000);
        schematic.metadata.modified = Some(2000);

        let stone = BlockState::new("minecraft:stone".to_string());
        // Place blocks at offset positions
        for x in 10..13 {
            for y in 20..23 {
                for z in 30..33 {
                    schematic.set_block(x, y, z, &stone);
                }
            }
        }

        // Add block entities at specific positions
        let chest1 = BlockEntity::new("minecraft:chest".to_string(), (10, 20, 30));
        let chest2 = BlockEntity::new("minecraft:chest".to_string(), (12, 22, 32));
        schematic.default_region.add_block_entity(chest1);
        schematic.default_region.add_block_entity(chest2);

        // Add entities
        let creeper = Entity::new("minecraft:creeper".to_string(), (11.5, 21.0, 31.5));
        schematic.default_region.add_entity(creeper);

        // Roundtrip
        let litematic_data = to_litematic(&schematic).unwrap();
        let roundtrip = from_litematic(&litematic_data).unwrap();

        let rt_region = roundtrip
            .get_region(&roundtrip.default_region_name)
            .unwrap();

        // Block entities should preserve absolute positions
        assert_eq!(rt_region.block_entities.len(), 2);
        assert!(
            rt_region.block_entities.contains_key(&(10, 20, 30)),
            "Block entity at (10,20,30) should exist after roundtrip"
        );
        assert!(
            rt_region.block_entities.contains_key(&(12, 22, 32)),
            "Block entity at (12,22,32) should exist after roundtrip"
        );

        // Entities should preserve absolute positions
        assert_eq!(rt_region.entities.len(), 1);
        let rt_entity = &rt_region.entities[0];
        assert!(
            (rt_entity.position.0 - 11.5).abs() < 0.001,
            "Entity X should be 11.5, got {}",
            rt_entity.position.0
        );
        assert!(
            (rt_entity.position.1 - 21.0).abs() < 0.001,
            "Entity Y should be 21.0, got {}",
            rt_entity.position.1
        );
        assert!(
            (rt_entity.position.2 - 31.5).abs() < 0.001,
            "Entity Z should be 31.5, got {}",
            rt_entity.position.2
        );

        // Blocks should also be preserved
        for x in 10..13 {
            for y in 20..23 {
                for z in 30..33 {
                    assert_eq!(
                        rt_region.get_block(x, y, z).map(|b| b.name.as_str()),
                        Some("minecraft:stone"),
                        "Block at ({},{},{}) should be stone",
                        x,
                        y,
                        z
                    );
                }
            }
        }
    }
}
