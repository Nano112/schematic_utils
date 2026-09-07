use smol_str::SmolStr;
use std::collections::HashMap;
use std::fmt;
#[cfg(test)]
use std::io::BufReader;
use std::io::Read;

use crate::block_entity::BlockEntity;
use crate::entity::Entity;
use crate::formats::error::Result;
use crate::region::Region;
use crate::{BlockState, UniversalSchematic};
#[cfg(test)]
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
#[cfg(test)]
use quartz_nbt::io::{read_nbt, Flavor};
use quartz_nbt::{NbtCompound, NbtList, NbtTag};

// enum for versions of schematics
#[derive(Debug, Clone, Copy)]
pub enum SchematicVersion {
    V2,
    V3,
}

impl SchematicVersion {
    pub fn as_str(&self) -> &str {
        match self {
            SchematicVersion::V2 => "v2",
            SchematicVersion::V3 => "v3",
        }
    }

    pub fn from_str(version: &str) -> Option<SchematicVersion> {
        match version {
            "v2" => Some(SchematicVersion::V2),
            "v3" => Some(SchematicVersion::V3),
            _ => None,
        }
    }

    pub fn get_default() -> SchematicVersion {
        SchematicVersion::V3
    }

    pub fn get_all() -> Vec<SchematicVersion> {
        vec![SchematicVersion::V2, SchematicVersion::V3]
    }
}
impl fmt::Display for SchematicVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub fn is_schematic(data: &[u8]) -> bool {
    let root = match crate::formats::limits::parse_gzip_nbt(
        data,
        &crate::formats::limits::DecodeLimits::default(),
    ) {
        Ok(result) => result,
        Err(_) => {
            return false;
        }
    };

    //things should be under Schematic tag if not treat root as the schematic
    let root = root.get::<_, &NbtCompound>("Schematic").unwrap_or(&root);

    // get tge version of the schematic
    let version = root.get::<_, i32>("Version");
    if version.is_err() {
        return root.get::<_, &NbtCompound>("Blocks").is_ok();
    }

    // Check if it's a v3 schematic (which has a Blocks compound)
    if version.unwrap() == 3 {
        return root.get::<_, &NbtCompound>("Blocks").is_ok();
    }

    // Otherwise check for v2 format
    root.get::<_, i32>("DataVersion").is_ok()
        && root.get::<_, i16>("Width").is_ok()
        && root.get::<_, i16>("Height").is_ok()
        && root.get::<_, i16>("Length").is_ok()
        && root.get::<_, &Vec<i8>>("BlockData").is_ok()
}

/// Default compression level for schematic serialization.
/// Level 3 balances speed (~2x faster than L6) with size (~15% larger than L6).
const DEFAULT_COMPRESSION: Compression = Compression::new(3);

// Default function uses v3 format
pub fn to_schematic(schematic: &UniversalSchematic) -> Result<Vec<u8>> {
    to_schematic_version(schematic, SchematicVersion::get_default())
}

pub fn to_schematic_version(
    schematic: &UniversalSchematic,
    version: SchematicVersion,
) -> Result<Vec<u8>> {
    to_schematic_with_options(schematic, version, DEFAULT_COMPRESSION)
}

pub fn to_schematic_with_options(
    schematic: &UniversalSchematic,
    version: SchematicVersion,
    compression: Compression,
) -> Result<Vec<u8>> {
    match version {
        SchematicVersion::V2 => to_schematic_v2(schematic, compression),
        SchematicVersion::V3 => to_schematic_v3(schematic, compression),
    }
}

// Version 3 format (recommended)
fn to_schematic_v3(schematic: &UniversalSchematic, compression: Compression) -> Result<Vec<u8>> {
    let mut schematic_data = NbtCompound::new();

    // Version 3 format
    schematic_data.insert("Version", NbtTag::Int(3));
    schematic_data.insert(
        "DataVersion",
        NbtTag::Int(schematic.metadata.mc_version.unwrap_or(1343)),
    );

    // Borrow the common single, already-compact region. Only allocate for
    // actual multi-region merging or when empty padding must be removed.
    let merged_region_storage;
    let merged_region = if schematic.other_regions.is_empty() {
        &schematic.default_region
    } else {
        merged_region_storage = schematic.get_merged_region();
        &merged_region_storage
    };
    let compact_region_storage;
    let compact_region = if merged_region.is_content_compact() {
        merged_region
    } else {
        compact_region_storage = merged_region.to_compact();
        &compact_region_storage
    };

    let (width, height, length) = compact_region.get_dimensions();
    let offset_pos = compact_region.position;

    schematic_data.insert("Width", NbtTag::Short((width as i16).abs()));
    schematic_data.insert("Height", NbtTag::Short((height as i16).abs()));
    schematic_data.insert("Length", NbtTag::Short((length as i16).abs()));

    // Set offset to the minimum position of the compact region
    let offset = vec![offset_pos.0, offset_pos.1, offset_pos.2];
    schematic_data.insert("Offset", NbtTag::IntArray(offset));

    // Create the Blocks container (required in v3)
    let mut blocks_container = NbtCompound::new();

    // Create clean palette and mapping from compact region
    let (palette_nbt, palette_mapping) = convert_palette_with_mapping(&compact_region.palette);

    // Store palette size before moving palette_nbt
    let _palette_size = palette_nbt.len();
    blocks_container.insert("Palette", palette_nbt);

    // Remap directly into the encoded stream instead of materializing a
    // second u32 buffer proportional to the schematic volume.
    let mut block_data: Vec<u8> = Vec::with_capacity(compact_region.blocks.len() * 2);
    for &original_id in &compact_region.blocks {
        let block_id = palette_mapping.get(original_id).copied().unwrap_or(0) as u32;
        encode_varint_into(block_id, &mut block_data);
    }

    // Add block data to Blocks container (renamed from "BlockData" to "Data" in v3)
    // SAFETY: u8 and i8 have identical size, alignment, and representation
    let block_data_i8: Vec<i8> = unsafe {
        let mut v = std::mem::ManuallyDrop::new(block_data);
        Vec::from_raw_parts(v.as_mut_ptr() as *mut i8, v.len(), v.capacity())
    };
    blocks_container.insert("Data", NbtTag::ByteArray(block_data_i8));

    // Add block entities from compact region (using v3 format)
    let block_entities = convert_block_entities_v3(&compact_region, schematic.metadata.mc_version);
    blocks_container.insert("BlockEntities", NbtTag::List(block_entities));

    // Entities are stored at root (Schematic) level in v3 per Sponge spec.
    let entities = convert_entities_v3(&compact_region);

    // Add the Blocks container to schematic data
    schematic_data.insert("Blocks", NbtTag::Compound(blocks_container));

    schematic_data.insert("Entities", NbtTag::List(entities));

    // Add metadata
    let mut metadata_tag = schematic.metadata.to_nbt();
    if !schematic.definition_regions.is_empty() {
        if let NbtTag::Compound(ref mut metadata_compound) = metadata_tag {
            if let Ok(json) = serde_json::to_string(&schematic.definition_regions) {
                metadata_compound.insert("NucleationDefinitions", NbtTag::String(json));
            }
        }
    }
    // The embedded cell contract, beside NucleationDefinitions: a saved cell
    // is one artifact, schematic + contract, autodetected on open.
    if let Some(contract) = &schematic.metadata.cell_contract {
        if let NbtTag::Compound(ref mut metadata_compound) = metadata_tag {
            metadata_compound.insert("NucleationCellContract", NbtTag::String(contract.clone()));
        }
    }
    schematic_data.insert("Metadata", metadata_tag);

    // Create the proper root structure with "Schematic" tag
    let mut root = NbtCompound::new();
    root.insert("Schematic", NbtTag::Compound(schematic_data));

    // The test the build carries, at the root beside `Schematic` — the same
    // placement and reasoning as the `.litematic` writer: Sponge readers walk
    // the `Schematic` compound and ignore unknown root tags, so this survives
    // other tools, and it cannot be silently dropped by a Metadata rebuild.
    if let Some(spec) = &schematic.metadata.embedded_test {
        let mut test = NbtCompound::new();
        test.insert("Format", NbtTag::Int(super::NUCLEATION_TEST_FORMAT));
        test.insert("Spec", NbtTag::String(spec.clone()));
        root.insert("NucleationTest", NbtTag::Compound(test));
    }

    crate::nbt::canonicalize_compound(&mut root);
    let mut encoder = GzEncoder::new(Vec::new(), compression);
    quartz_nbt::io::write_nbt(
        &mut encoder,
        None,
        &root,
        quartz_nbt::io::Flavor::Uncompressed,
    )?;
    Ok(encoder.finish()?)
}

// Version 2 format (legacy compatibility)
fn to_schematic_v2(schematic: &UniversalSchematic, compression: Compression) -> Result<Vec<u8>> {
    let mut schematic_data = NbtCompound::new();

    schematic_data.insert("Version", NbtTag::Int(2)); // Schematic format version 2
    schematic_data.insert(
        "DataVersion",
        NbtTag::Int(schematic.metadata.mc_version.unwrap_or(1343)),
    );

    let merged_region_storage;
    let merged_region = if schematic.other_regions.is_empty() {
        &schematic.default_region
    } else {
        merged_region_storage = schematic.get_merged_region();
        &merged_region_storage
    };
    let compact_region_storage;
    let compact_region = if merged_region.is_content_compact() {
        merged_region
    } else {
        compact_region_storage = merged_region.to_compact();
        &compact_region_storage
    };

    let (width, height, length) = compact_region.get_dimensions();
    let offset_pos = compact_region.position;

    schematic_data.insert("Width", NbtTag::Short((width as i16).abs()));
    schematic_data.insert("Height", NbtTag::Short((height as i16).abs()));
    schematic_data.insert("Length", NbtTag::Short((length as i16).abs()));

    schematic_data.insert("Size", NbtTag::IntArray(vec![width, height, length]));

    // Set offset to the minimum position of the compact region
    let offset = vec![offset_pos.0, offset_pos.1, offset_pos.2];
    schematic_data.insert("Offset", NbtTag::IntArray(offset));

    let (palette_nbt, max_id) = convert_palette_v2(&compact_region.palette);
    schematic_data.insert("Palette", palette_nbt);
    schematic_data.insert("PaletteMax", max_id + 1);

    // Encode block data — preallocated to avoid per-block Vec allocations
    let mut block_data: Vec<u8> = Vec::with_capacity(compact_region.blocks.len() * 2);
    for &block_id in &compact_region.blocks {
        encode_varint_into(block_id as u32, &mut block_data);
    }

    // SAFETY: u8 and i8 have identical size, alignment, and representation
    let block_data_i8: Vec<i8> = unsafe {
        let mut v = std::mem::ManuallyDrop::new(block_data);
        Vec::from_raw_parts(v.as_mut_ptr() as *mut i8, v.len(), v.capacity())
    };
    schematic_data.insert("BlockData", NbtTag::ByteArray(block_data_i8));

    // Use block entities and entities from compact region
    let block_entities = convert_block_entities(&compact_region);
    let entities = convert_entities_v2(&compact_region);

    schematic_data.insert("BlockEntities", NbtTag::List(block_entities));
    schematic_data.insert("Entities", NbtTag::List(entities));

    // Add metadata
    let mut metadata_tag = schematic.metadata.to_nbt();
    if !schematic.definition_regions.is_empty() {
        if let NbtTag::Compound(ref mut metadata_compound) = metadata_tag {
            if let Ok(json) = serde_json::to_string(&schematic.definition_regions) {
                metadata_compound.insert("NucleationDefinitions", NbtTag::String(json));
            }
        }
    }
    // The embedded cell contract, beside NucleationDefinitions: a saved cell
    // is one artifact, schematic + contract, autodetected on open.
    if let Some(contract) = &schematic.metadata.cell_contract {
        if let NbtTag::Compound(ref mut metadata_compound) = metadata_tag {
            metadata_compound.insert("NucleationCellContract", NbtTag::String(contract.clone()));
        }
    }
    schematic_data.insert("Metadata", metadata_tag);

    // Create the proper root structure with "Schematic" tag
    let mut root = NbtCompound::new();
    root.insert("Schematic", NbtTag::Compound(schematic_data));

    // The test the build carries, at the root beside `Schematic` — the same
    // placement and reasoning as the `.litematic` writer: Sponge readers walk
    // the `Schematic` compound and ignore unknown root tags, so this survives
    // other tools, and it cannot be silently dropped by a Metadata rebuild.
    if let Some(spec) = &schematic.metadata.embedded_test {
        let mut test = NbtCompound::new();
        test.insert("Format", NbtTag::Int(super::NUCLEATION_TEST_FORMAT));
        test.insert("Spec", NbtTag::String(spec.clone()));
        root.insert("NucleationTest", NbtTag::Compound(test));
    }

    crate::nbt::canonicalize_compound(&mut root);
    let mut encoder = GzEncoder::new(Vec::new(), compression);
    quartz_nbt::io::write_nbt(
        &mut encoder,
        None,
        &root,
        quartz_nbt::io::Flavor::Uncompressed,
    )?;
    Ok(encoder.finish()?)
}

// Palette conversion for v3 (creates clean sequential indices)
fn convert_palette(palette: &Vec<BlockState>) -> (NbtCompound, i32) {
    let (nbt_palette, _) = convert_palette_with_mapping(palette);
    let max_id = nbt_palette.len() as i32 - 1;
    (nbt_palette, max_id)
}

// Helper function that returns both palette and mapping for index conversion
fn convert_palette_with_mapping(palette: &Vec<BlockState>) -> (NbtCompound, Vec<i32>) {
    let mut nbt_palette = NbtCompound::new();
    let mut mapping = vec![0i32; palette.len()]; // Default all to air (index 0)

    // Always start with air at index 0
    nbt_palette.insert("minecraft:air", NbtTag::Int(0));
    let mut next_id = 1;

    for (original_id, block_state) in palette.iter().enumerate() {
        // Handle invalid or unknown blocks by mapping them to air
        if block_state.name.is_empty() || block_state.name == "minecraft:unknown" {
            mapping[original_id] = 0; // Map to air
            continue;
        }

        // If it's already air, map to index 0
        if block_state.name == "minecraft:air" {
            mapping[original_id] = 0;
            continue;
        }

        let key: String = if block_state.properties.is_empty() {
            block_state.name.to_string()
        } else {
            format!(
                "{}[{}]",
                block_state.name,
                block_state
                    .properties
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };

        // Check if this block state already exists in the palette
        let mut found_id = None;
        for (existing_key, tag) in nbt_palette.inner() {
            if existing_key == &key {
                if let NbtTag::Int(id) = tag {
                    found_id = Some(*id);
                    break;
                }
            }
        }

        let assigned_id = if let Some(id) = found_id {
            id
        } else {
            nbt_palette.insert(&key, NbtTag::Int(next_id));
            let id = next_id;
            next_id += 1;
            id
        };

        mapping[original_id] = assigned_id;
    }

    (nbt_palette, mapping)
}

// Palette conversion for v2 (legacy behavior with air at index 0)
fn convert_palette_v2(palette: &Vec<BlockState>) -> (NbtCompound, i32) {
    let mut nbt_palette = NbtCompound::new();
    let mut max_id = 0;

    // Always ensure air is at index 0
    nbt_palette.insert("minecraft:air", NbtTag::Int(0));

    let mut next_id = 1; // Start at 1 since air is at 0

    for block_state in palette.iter() {
        if block_state.name == "minecraft:air" {
            continue; // Skip air blocks as we already added it at index 0
        }

        let key: String = if block_state.properties.is_empty() {
            block_state.name.to_string()
        } else {
            format!(
                "{}[{}]",
                block_state.name,
                block_state
                    .properties
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };

        nbt_palette.insert(&key, NbtTag::Int(next_id));
        max_id = max_id.max(next_id);
        next_id += 1;
    }

    (nbt_palette, max_id)
}
pub fn from_schematic(data: &[u8]) -> Result<UniversalSchematic> {
    from_schematic_bounded(data, &crate::formats::limits::DecodeLimits::default())
}

pub fn from_schematic_bounded(
    data: &[u8],
    limits: &crate::formats::limits::DecodeLimits,
) -> Result<UniversalSchematic> {
    let root = crate::formats::limits::parse_gzip_nbt(data, limits)?;

    let schem = root.get::<_, &NbtCompound>("Schematic").unwrap_or(&root);
    let schem_version = schem.get::<_, i32>("Version")?;

    // The test the build carries, if any. On the *outer* root, beside
    // `Schematic` — see the writer for why. An unknown `Format` is read
    // anyway: the descriptor is JSON and the runner reports what it cannot
    // parse, which beats a file that silently claims to have no test.
    let embedded_test = root
        .get::<_, &NbtCompound>("NucleationTest")
        .ok()
        .and_then(|test| test.get::<_, &str>("Spec").ok().map(String::from));

    let mut definition_regions = HashMap::new();

    // The full Metadata compound, not just Name: our own writer emits Author
    // and Description there (and the Sponge v2/v3 spec puts them there), so
    // dropping them on read made `.litematic -> .schem` lose attribution.
    let mut file_metadata = crate::metadata::Metadata::default();
    let mut cell_contract = None;
    if let Ok(metadata) = schem.get::<_, &NbtCompound>("Metadata") {
        if let Ok(json) = metadata.get::<_, &str>("NucleationDefinitions") {
            if let Ok(regions) = serde_json::from_str(json) {
                definition_regions = regions;
            }
        }
        if let Ok(json) = metadata.get::<_, &str>("NucleationCellContract") {
            cell_contract = Some(json.to_string());
        }
        if let Ok(parsed) = crate::metadata::Metadata::from_nbt(metadata) {
            file_metadata = parsed;
        }
    }
    let name = file_metadata
        .name
        .clone()
        .unwrap_or_else(|| "Unnamed".to_string());

    let mc_version = schem.get::<_, i32>("DataVersion").ok();

    let mut schematic = UniversalSchematic::new(name.clone());
    schematic.metadata = file_metadata;
    schematic.metadata.name = Some(name);
    schematic.definition_regions = definition_regions;
    schematic.metadata.embedded_test = embedded_test;
    schematic.metadata.cell_contract = cell_contract;
    // The root DataVersion is authoritative when present; a Metadata-carried
    // mc_version (our own writer emits one) fills in otherwise.
    schematic.metadata.mc_version = mc_version.or(schematic.metadata.mc_version);
    // The Sponge `DataVersion` is the file's source version for conversion.
    schematic.metadata.source_data_version = mc_version;

    let width = schem.get::<_, i16>("Width")? as u32;
    let height = schem.get::<_, i16>("Height")? as u32;
    let length = schem.get::<_, i16>("Length")? as u32;
    limits.check_dimensions((i64::from(width), i64::from(height), i64::from(length)))?;

    let block_container = if schem_version == 2 {
        schem
    } else {
        schem.get::<_, &NbtCompound>("Blocks")?
    };

    let palette_len = block_container.get::<_, &NbtCompound>("Palette")?.len();
    if palette_len > limits.max_palette_entries {
        return Err("palette limit exceeded".into());
    }
    if block_container
        .get::<_, &NbtList>("BlockEntities")
        .is_ok_and(|values| values.len() > limits.max_block_entities)
    {
        return Err("block-entity limit exceeded".into());
    }
    if schem
        .get::<_, &NbtList>("Entities")
        .is_ok_and(|values| values.len() > limits.max_entities)
    {
        return Err("entity limit exceeded".into());
    }

    let block_palette = parse_block_palette(block_container)?;

    let block_data = parse_block_data(block_container, width, height, length)?;

    // Adopt the decoded indices directly. Constructing a full zero-filled
    // region and converting a second u32 vector used three volume-sized
    // buffers simultaneously (over 3 GiB for a large wasm32 schematic).
    let mut region = Region::new("Main".to_string(), (0, 0, 0), (1, 1, 1));
    region.size = (width as i32, height as i32, length as i32);
    region.palette = block_palette;
    region.blocks = block_data;

    // Rebuild caches after directly setting palette and blocks
    region.rebuild_bbox();
    region.rebuild_palette_index();
    region.rebuild_air_index();
    region.rebuild_non_air_count();
    region.rebuild_tight_bounds();

    let block_entities = parse_block_entities(block_container)?;
    for block_entity in block_entities {
        region.add_block_entity(block_entity);
    }

    let entities = parse_entities(schem)?;
    for entity in entities {
        region.add_entity(entity);
    }

    schematic.add_region(region);
    limits.validate_schematic(&schematic)?;
    Ok(schematic)
}

// Sponge Schematic spec: block entity positions are relative to [0,0,0] of the schematic
// (without the offset applied). We subtract the region position (which becomes the offset).
fn convert_block_entities(region: &Region) -> NbtList {
    let mut block_entities = NbtList::new();

    for (_, block_entity) in region.block_entities.iter() {
        let mut nbt = block_entity.to_nbt();
        let rel_x = block_entity.position.0 - region.position.0;
        let rel_y = block_entity.position.1 - region.position.1;
        let rel_z = block_entity.position.2 - region.position.2;
        nbt.insert("Pos", NbtTag::IntArray(vec![rel_x, rel_y, rel_z]));
        block_entities.push(nbt);
    }

    block_entities
}

// Convert block entities for Sponge Schematic v3 format
// Uses to_nbt_v3() which wraps block-specific data in a "Data" compound
// Positions are relative to [0,0,0] of the schematic (without offset applied)
fn convert_block_entities_v3(region: &Region, data_version: Option<i32>) -> NbtList {
    let mut block_entities = NbtList::new();

    for (_, block_entity) in region.block_entities.iter() {
        let mut nbt = block_entity.to_nbt_v3(data_version);
        let rel_x = block_entity.position.0 - region.position.0;
        let rel_y = block_entity.position.1 - region.position.1;
        let rel_z = block_entity.position.2 - region.position.2;
        nbt.insert("Pos", NbtTag::IntArray(vec![rel_x, rel_y, rel_z]));
        block_entities.push(nbt);
    }

    block_entities
}

// Sponge Schematic spec: entity positions are relative to [0,0,0] of the
// schematic (without the offset applied). We subtract the region position.

fn sponge_entity_id(entity: &Entity) -> String {
    if entity.id.starts_with("minecraft:") {
        entity.id.clone()
    } else {
        format!("minecraft:{}", entity.id)
    }
}

// Vanilla MC chunk-format entity NBT, with Pos rewritten to be relative to
// the schematic origin and with Motion/Rotation defaults filled in (real
// loaders like WorldEdit reject entities missing these fields).
fn vanilla_entity_nbt(entity: &Entity, rel_pos: (f64, f64, f64)) -> NbtCompound {
    let mut nbt = if let NbtTag::Compound(c) = entity.to_nbt() {
        c
    } else {
        NbtCompound::new()
    };

    let pos_list = NbtList::from(vec![
        NbtTag::Double(rel_pos.0),
        NbtTag::Double(rel_pos.1),
        NbtTag::Double(rel_pos.2),
    ]);
    nbt.insert("Pos", NbtTag::List(pos_list));

    if !nbt.inner().contains_key("Motion") {
        let motion = NbtList::from(vec![
            NbtTag::Double(0.0),
            NbtTag::Double(0.0),
            NbtTag::Double(0.0),
        ]);
        nbt.insert("Motion", NbtTag::List(motion));
    }
    if !nbt.inner().contains_key("Rotation") {
        let rotation = NbtList::from(vec![NbtTag::Float(0.0), NbtTag::Float(0.0)]);
        nbt.insert("Rotation", NbtTag::List(rotation));
    }

    nbt
}

// Sponge v2 entity layout: vanilla MC fields flat at top level, with the
// entity-type resource location under `Id` (capitalised).
fn convert_entities_v2(region: &Region) -> NbtList {
    let mut entities = NbtList::new();
    for entity in &region.entities {
        let rel = (
            entity.position.0 - region.position.0 as f64,
            entity.position.1 - region.position.1 as f64,
            entity.position.2 - region.position.2 as f64,
        );
        let mut nbt = vanilla_entity_nbt(entity, rel);
        nbt.insert("Id", NbtTag::String(sponge_entity_id(entity)));
        entities.push(NbtTag::Compound(nbt));
    }
    entities
}

// Sponge v3 entity layout: top-level { Id, Pos, Data }, where Data is the
// vanilla MC chunk-format entity NBT.
fn convert_entities_v3(region: &Region) -> NbtList {
    let mut entities = NbtList::new();
    for entity in &region.entities {
        let rel = (
            entity.position.0 - region.position.0 as f64,
            entity.position.1 - region.position.1 as f64,
            entity.position.2 - region.position.2 as f64,
        );
        let data = vanilla_entity_nbt(entity, rel);

        let pos_list = NbtList::from(vec![
            NbtTag::Double(rel.0),
            NbtTag::Double(rel.1),
            NbtTag::Double(rel.2),
        ]);

        let mut wrapper = NbtCompound::new();
        wrapper.insert("Id", NbtTag::String(sponge_entity_id(entity)));
        wrapper.insert("Pos", NbtTag::List(pos_list));
        wrapper.insert("Data", NbtTag::Compound(data));
        entities.push(NbtTag::Compound(wrapper));
    }
    entities
}

fn parse_block_palette(region_tag: &NbtCompound) -> Result<Vec<BlockState>> {
    let palette_compound = region_tag.get::<_, &NbtCompound>("Palette")?;
    let palette_max = region_tag
        .get::<_, i32>("PaletteMax") // V2
        .unwrap_or(palette_compound.len() as i32) as usize; // V3
    let mut palette = vec![BlockState::new("minecraft:air".to_string()); palette_max + 1];

    for (block_state_str, value) in palette_compound.inner() {
        if let NbtTag::Int(id) = value {
            let block_state = parse_block_state(block_state_str);
            palette[*id as usize] = block_state;
        }
    }

    Ok(palette)
}

fn parse_block_state(input: &str) -> BlockState {
    if let Some((name, properties_str)) = input.split_once('[') {
        let name: SmolStr = name.into();
        let properties = properties_str
            .trim_end_matches(']')
            .split(',')
            .filter_map(|prop| {
                let mut parts = prop.splitn(2, '=');
                Some((
                    SmolStr::from(parts.next()?.trim()),
                    SmolStr::from(parts.next()?.trim()),
                ))
            })
            .collect();
        BlockState { name, properties }
    } else {
        BlockState::new(input.to_string())
    }
}

pub fn encode_varint(value: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    encode_varint_into(value, &mut bytes);
    bytes
}

#[inline]
fn encode_varint_into(value: u32, buf: &mut Vec<u8>) {
    let mut val = value;
    loop {
        let mut byte = (val & 0b0111_1111) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0b1000_0000;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

fn decode_varint<R: Read>(reader: &mut R) -> Result<u32> {
    let mut result = 0u32;
    let mut shift = 0;
    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        result |= ((byte[0] & 0b0111_1111) as u32) << shift;
        if byte[0] & 0b1000_0000 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift >= 32 {
            return Err("Varint is too long".into());
        }
    }
}

fn parse_block_data(
    region_tag: &NbtCompound,
    width: u32,
    height: u32,
    length: u32,
) -> Result<Vec<usize>> {
    // V2 = BlockData, V3 = Data
    let block_data_i8 = region_tag
        .get::<_, &Vec<i8>>("BlockData")
        .or(region_tag.get::<_, &Vec<i8>>("Data"))?;

    let mut block_data_u8: &[u8] = unsafe {
        std::slice::from_raw_parts(block_data_i8.as_ptr() as *const u8, block_data_i8.len())
    };

    // ---------- fast var-int decode ----------
    #[inline]
    fn read_varint(slice: &mut &[u8]) -> Option<u32> {
        let mut out = 0u32;
        let mut shift = 0;
        while !slice.is_empty() {
            let byte = slice[0];
            *slice = &slice[1..];
            if shift == 28 && byte > 0x0f {
                return None;
            }
            out |= ((byte & 0x7F) as u32) << shift;
            if byte & 0x80 == 0 {
                return Some(out);
            }
            shift += 7;
        }
        None
    }

    let expected_length = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(length as usize))
        .ok_or("Block data volume overflow")?;
    // Every index needs at least one byte. Reject truncated data before
    // reserving an output buffer, and never grow beyond the validated volume.
    if block_data_u8.len() < expected_length {
        return Err("Block data length mismatch: truncated indices".into());
    }
    let mut block_data = Vec::new();
    block_data
        .try_reserve_exact(expected_length)
        .map_err(|error| format!("Cannot allocate schematic block data: {error}"))?;
    for _ in 0..expected_length {
        let id = read_varint(&mut block_data_u8)
            .ok_or("Block data contains a truncated or invalid varint")?;
        block_data.push(id as usize);
    }
    if !block_data_u8.is_empty() {
        return Err("Block data length mismatch: excess indices".into());
    }

    Ok(block_data)
}

fn parse_block_entities(region_tag: &NbtCompound) -> Result<Vec<BlockEntity>> {
    if !region_tag.contains_key("BlockEntities") {
        return Ok(Vec::new());
    }
    let block_entities_list = region_tag.get::<_, &NbtList>("BlockEntities")?;
    let mut block_entities = Vec::new();

    for tag in block_entities_list.iter() {
        if let NbtTag::Compound(compound) = tag {
            // Sponge Schematic v3 wraps block-specific data in a "Data" compound.
            // Flatten it so consumers (e.g. MCHPRS) can find fields like "Items"
            // at the top level, matching the vanilla block entity NBT layout.
            let flattened = if compound.contains_key("Data") {
                let mut flat = NbtCompound::new();
                // Copy top-level fields (Id, Pos)
                for (key, value) in compound.inner() {
                    if key != "Data" {
                        flat.insert(key, value.clone());
                    }
                }
                // Merge Data contents into top level
                if let Ok(data) = compound.get::<_, &NbtCompound>("Data") {
                    for (key, value) in data.inner() {
                        flat.insert(key, value.clone());
                    }
                }
                flat
            } else {
                compound.clone()
            };
            let block_entity = BlockEntity::from_nbt(&flattened);
            block_entities.push(block_entity);
        }
    }

    Ok(block_entities)
}

fn parse_entities(region_tag: &NbtCompound) -> Result<Vec<Entity>> {
    if !region_tag.contains_key("Entities") {
        return Ok(Vec::new());
    }
    let entities_list = region_tag.get::<_, &NbtList>("Entities")?;
    let mut entities = Vec::new();

    for tag in entities_list.iter() {
        if let NbtTag::Compound(compound) = tag {
            entities.push(parse_entity_compound(compound)?);
        }
    }

    Ok(entities)
}

// Sponge v3 wraps the vanilla MC entity NBT in a `Data` sub-compound with
// `Id`/`Pos` hoisted to the top. v2 and legacy Nucleation output put the
// vanilla NBT directly at the top level. This accepts both shapes.
fn parse_entity_compound(compound: &NbtCompound) -> std::result::Result<Entity, String> {
    if let Ok(data) = compound.get::<_, &NbtCompound>("Data") {
        let mut merged = data.clone();

        // Top-level Id/Pos are authoritative per spec.
        if let Ok(top_id) = compound.get::<_, &str>("Id") {
            merged.insert("Id", NbtTag::String(top_id.to_string()));
        }
        if let Ok(top_pos) = compound.get::<_, &NbtList>("Pos") {
            merged.insert("Pos", NbtTag::List(top_pos.clone()));
        }

        Entity::from_nbt(&merged)
    } else {
        Entity::from_nbt(compound)
    }
}

use crate::formats::manager::{SchematicExporter, SchematicImporter};

pub struct SchematicFormat;

impl SchematicImporter for SchematicFormat {
    fn name(&self) -> String {
        "schematic".to_string()
    }

    fn detect(&self, data: &[u8]) -> bool {
        is_schematic(data)
    }

    fn detect_bounded(&self, data: &[u8], limits: &crate::formats::limits::DecodeLimits) -> bool {
        from_schematic_bounded(data, limits).is_ok()
    }

    fn read(&self, data: &[u8]) -> Result<UniversalSchematic> {
        from_schematic(data)
    }

    fn read_bounded(
        &self,
        data: &[u8],
        limits: &crate::formats::limits::DecodeLimits,
    ) -> Result<UniversalSchematic> {
        from_schematic_bounded(data, limits)
    }
}

impl SchematicExporter for SchematicFormat {
    fn name(&self) -> String {
        "schematic".to_string()
    }

    fn extensions(&self) -> Vec<String> {
        vec!["schem".to_string(), "schematic".to_string()]
    }

    fn available_versions(&self) -> Vec<String> {
        SchematicVersion::get_all()
            .iter()
            .map(|v| v.as_str().to_string())
            .collect()
    }

    fn default_version(&self) -> String {
        SchematicVersion::get_default().as_str().to_string()
    }

    fn write(&self, schematic: &UniversalSchematic, version: Option<&str>) -> Result<Vec<u8>> {
        if let Some(v) = version {
            match SchematicVersion::from_str(v) {
                Some(ver) => to_schematic_version(schematic, ver),
                None => Err(format!("Unsupported version: {}", v).into()),
            }
        } else {
            to_schematic(schematic)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::fs::File;
    use std::io::{Cursor, Write};
    use std::path::Path;

    use crate::litematic::{from_litematic, to_litematic};
    use crate::{BlockState, UniversalSchematic};

    use super::*;

    #[test]
    fn already_compact_single_region_round_trips_in_both_versions() {
        let mut original = UniversalSchematic::new("compact-export".into());
        original.fill_cuboid_str((0, 0, 0), (7, 5, 3), "minecraft:stone");
        assert!(original.default_region.is_content_compact());

        for version in [SchematicVersion::V2, SchematicVersion::V3] {
            let bytes = to_schematic_version(&original, version).unwrap();
            let decoded = from_schematic(&bytes).unwrap();
            assert_eq!(decoded.get_dimensions(), (8, 6, 4));
            assert_eq!(decoded.default_region.count_non_air_blocks(), 8 * 6 * 4);
            assert_eq!(decoded.get_block(7, 5, 3).unwrap().name, "minecraft:stone");
        }
    }

    /// A `.schem` that carries its own test keeps it across resaves, exactly
    /// as a `.litematic` does — one embedding, every carrier. Root-level and
    /// not inside `Metadata`, for the same reasons as the litematic writer.
    #[test]
    fn schematic_preserves_an_embedded_test_across_a_resave() {
        let spec = r#"{"name":"a door opens","checks":[{"tick":0,"expect":"quiescent"}]}"#;

        for version in [SchematicVersion::V2, SchematicVersion::V3] {
            let mut schem = UniversalSchematic::new("carrier".into());
            schem.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));
            schem.metadata.embedded_test = Some(spec.to_string());

            let first = to_schematic_version(&schem, version).expect("writes");
            let reloaded = from_schematic(&first).expect("reads");
            assert_eq!(
                reloaded.metadata.embedded_test.as_deref(),
                Some(spec),
                "{version:?}: the embedded test must survive one round trip"
            );

            let second = to_schematic_version(&reloaded, version).expect("writes again");
            let twice = from_schematic(&second).expect("reads again");
            assert_eq!(
                twice.metadata.embedded_test.as_deref(),
                Some(spec),
                "{version:?}: re-saving a loaded build must not drop its test"
            );

            // A build with no test writes no tag at all, rather than an empty one.
            let plain = to_schematic_version(&UniversalSchematic::new("plain".into()), version)
                .expect("writes");
            let back = from_schematic(&plain).expect("reads");
            assert_eq!(back.metadata.embedded_test, None, "{version:?}");
        }
    }

    /// Author and Description must survive our own `.schem` round trip.
    ///
    /// The writer always emitted `Metadata.{Name, Author, Description}`; the
    /// reader used to take only `Name` back, so `.litematic -> .schem`
    /// conversion silently dropped attribution (issue #7).
    #[test]
    fn schem_round_trip_keeps_author_and_description() {
        let mut schematic = UniversalSchematic::new("demo".to_string());
        schematic.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));
        schematic.metadata.author = Some("Notch".to_string());
        schematic.metadata.description = Some("desc-marker".to_string());
        schematic.metadata.created = Some(1_700_000_000_000);

        let bytes = to_schematic(&schematic).expect("write .schem");
        let back = from_schematic(&bytes).expect("read .schem");

        assert_eq!(back.metadata.name.as_deref(), Some("demo"));
        assert_eq!(back.metadata.author.as_deref(), Some("Notch"));
        assert_eq!(back.metadata.description.as_deref(), Some("desc-marker"));
        assert_eq!(back.metadata.created, Some(1_700_000_000_000));
    }

    /// A `.schem` with no attribution still reads, with the fields absent
    /// rather than invented.
    #[test]
    fn schem_without_attribution_reads_with_absent_fields() {
        let mut schematic = UniversalSchematic::new("plain".to_string());
        schematic.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));

        let bytes = to_schematic(&schematic).expect("write .schem");
        let back = from_schematic(&bytes).expect("read .schem");

        assert_eq!(back.metadata.name.as_deref(), Some("plain"));
        assert_eq!(back.metadata.author, None);
        assert_eq!(back.metadata.description, None);
    }

    /// Sponge v3 marks `BlockEntities` optional, and FAWE omits the key
    /// outright when a build has none. Our own writer always emits the list,
    /// so strip it back out to get the file those exports actually produce:
    /// the importer must read the blocks rather than error on the absent key.
    #[test]
    fn schem_v3_reads_a_region_that_omits_block_entities() {
        let mut schematic = UniversalSchematic::new("no block entities".to_string());
        schematic.set_block(0, 0, 0, &BlockState::new("minecraft:stone".to_string()));

        let bytes = to_schematic(&schematic).expect("write .schem");
        let (mut root, root_name) = {
            let mut gz = GzDecoder::new(BufReader::new(&bytes[..]));
            read_nbt(&mut gz, Flavor::Uncompressed).expect("decode .schem")
        };

        {
            let schem = root
                .get_mut::<_, &mut NbtCompound>("Schematic")
                .expect("Schematic compound");
            let blocks = schem
                .get::<_, &NbtCompound>("Blocks")
                .expect("Blocks compound");
            assert!(
                blocks.contains_key("BlockEntities"),
                "writer stopped emitting BlockEntities — this test no longer strips anything"
            );

            let mut stripped = NbtCompound::new();
            for (key, value) in blocks.inner().iter() {
                if key != "BlockEntities" {
                    stripped.insert(key.clone(), value.clone());
                }
            }
            schem.insert("Blocks", stripped);
        }

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        quartz_nbt::io::write_nbt(&mut encoder, Some(&root_name), &root, Flavor::Uncompressed)
            .expect("re-encode .schem");
        let without_block_entities = encoder.finish().expect("finish gzip");

        let back =
            from_schematic(&without_block_entities).expect("read .schem lacking BlockEntities");

        assert!(back.get_block_entities_as_list().is_empty());
        assert_eq!(
            back.get_block(0, 0, 0).map(|b| b.name.as_str()),
            Some("minecraft:stone")
        );
    }

    #[test]
    fn test_schematic_file_generation() {
        // Create a test schematic
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        for x in 0..5 {
            for y in 0..5 {
                for z in 0..5 {
                    if (x + y + z) % 2 == 0 {
                        schematic.set_block(x, y, z, &stone);
                    } else {
                        schematic.set_block(x, y, z, &dirt);
                    }
                }
            }
        }

        // Convert the schematic to .schem format
        let schem_data = to_schematic(&schematic).expect("Failed to convert schematic");

        // Save the .schem file
        let mut file = File::create("test_schematic.schem").expect("Failed to create file");
        file.write_all(&schem_data)
            .expect("Failed to write to file");

        // Read the .schem file back
        let loaded_schem_data = std::fs::read("test_schematic.schem").expect("Failed to read file");

        // Parse the loaded .schem data
        let loaded_schematic =
            from_schematic(&loaded_schem_data).expect("Failed to parse schematic");

        // Compare the original and loaded schematics
        assert_eq!(schematic.metadata.name, loaded_schematic.metadata.name);
        assert_eq!(
            schematic.other_regions.len(),
            loaded_schematic.other_regions.len()
        );
        // Compare tight dimensions (actual content) instead of allocated bounds
        // The export uses compact regions, so loaded schematic will have tight bounds
        assert_eq!(
            schematic.get_tight_dimensions(),
            loaded_schematic.get_dimensions() // Loaded will have tight bounds as its actual size
        );

        let original_region = schematic.default_region;
        let loaded_region = loaded_schematic.default_region;

        assert_eq!(original_region.entities.len(), loaded_region.entities.len());
        assert_eq!(
            original_region.block_entities.len(),
            loaded_region.block_entities.len()
        );

        // Clean up the generated file
        //std::fs::remove_file("test_schematic.schem").expect("Failed to remove file");
    }

    #[test]
    fn test_varint_encoding_decoding() {
        let test_cases = vec![
            0u32,
            1u32,
            127u32,
            128u32,
            255u32,
            256u32,
            65535u32,
            65536u32,
            4294967295u32,
        ];

        for &value in &test_cases {
            let encoded = encode_varint(value);

            let mut cursor = Cursor::new(encoded);
            let decoded = decode_varint(&mut cursor).unwrap();

            assert_eq!(
                value, decoded,
                "Encoding and decoding failed for value: {}",
                value
            );
        }
    }

    #[test]
    fn test_parse_block_data() {
        let mut nbt = NbtCompound::new();
        let block_data = [0, 1, 2, 1, 0, 2, 1, 0]; // 8 blocks
        let encoded_block_data: Vec<u8> =
            block_data.iter().flat_map(|&v| encode_varint(v)).collect();

        nbt.insert(
            "BlockData",
            NbtTag::ByteArray(encoded_block_data.iter().map(|&x| x as i8).collect()),
        );

        let parsed_data = parse_block_data(&nbt, 2, 2, 2).expect("Failed to parse block data");
        assert_eq!(parsed_data, vec![0, 1, 2, 1, 0, 2, 1, 0]);
    }

    #[test]
    fn block_data_checks_multibyte_indices_and_declared_volume() {
        let mut nbt = NbtCompound::new();
        for (bytes, expected) in [
            (vec![0, 127, 0x80, 1, 0xac, 2], Some(vec![0, 127, 128, 300])),
            (vec![0, 1, 2], None),
            (vec![0, 1, 2, 3, 4], None),
            (vec![0, 1, 2, 0x80], None),
            (vec![0, 1, 2, 0xff, 0xff, 0xff, 0xff, 0x10], None),
        ] {
            nbt.insert(
                "Data",
                NbtTag::ByteArray(bytes.into_iter().map(|v| v as i8).collect()),
            );
            assert_eq!(parse_block_data(&nbt, 2, 1, 2).ok(), expected);
        }
    }

    #[test]
    fn test_convert_palette_v3() {
        let palette = vec![
            BlockState::new("minecraft:stone".to_string()),
            BlockState::new("minecraft:dirt".to_string()),
            BlockState {
                name: "minecraft:wool".into(),
                properties: vec![("color".into(), "red".into())],
            },
        ];

        let (nbt_palette, max_id) = convert_palette(&palette);

        // V3 now ensures air is always at index 0 for WorldEdit compatibility
        assert_eq!(max_id, 3); // air=0, stone=1, dirt=2, wool=3
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:air").unwrap(), 0);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:stone").unwrap(), 1);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:dirt").unwrap(), 2);
        assert_eq!(
            nbt_palette
                .get::<_, i32>("minecraft:wool[color=red]")
                .unwrap(),
            3
        );
    }

    #[test]
    fn test_convert_palette_v2() {
        let palette = vec![
            BlockState::new("minecraft:stone".to_string()),
            BlockState::new("minecraft:dirt".to_string()),
            BlockState {
                name: "minecraft:wool".into(),
                properties: vec![("color".into(), "red".into())],
            },
        ];

        let (nbt_palette, max_id) = convert_palette_v2(&palette);

        // V2 behavior: Air is always at index 0, other blocks follow
        assert_eq!(max_id, 3); // Air=0, stone=1, dirt=2, wool=3
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:air").unwrap(), 0);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:stone").unwrap(), 1);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:dirt").unwrap(), 2);
        assert_eq!(
            nbt_palette
                .get::<_, i32>("minecraft:wool[color=red]")
                .unwrap(),
            3
        );
    }

    #[test]
    fn test_convert_palette_v3_with_air() {
        let palette = vec![
            BlockState::new("minecraft:air".to_string()),
            BlockState::new("minecraft:stone".to_string()),
            BlockState::new("minecraft:dirt".to_string()),
        ];

        let (nbt_palette, max_id) = convert_palette(&palette);

        // V3 with air explicitly in palette - air should still be at index 0
        assert_eq!(max_id, 2);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:air").unwrap(), 0);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:stone").unwrap(), 1);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:dirt").unwrap(), 2);
    }

    #[test]
    fn test_convert_palette_with_mapping() {
        let palette = vec![
            BlockState::new("minecraft:stone".to_string()),
            BlockState::new("minecraft:unknown".to_string()), // Should be mapped to air
            BlockState::new("minecraft:dirt".to_string()),
        ];

        let (nbt_palette, mapping) = convert_palette_with_mapping(&palette);

        // Check palette structure
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:air").unwrap(), 0);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:stone").unwrap(), 1);
        assert_eq!(nbt_palette.get::<_, i32>("minecraft:dirt").unwrap(), 2);

        // Check mapping array
        assert_eq!(mapping[0], 1); // stone -> 1
        assert_eq!(mapping[1], 0); // unknown -> 0 (air)
        assert_eq!(mapping[2], 2); // dirt -> 2
    }
    #[test]
    fn test_import_new_chest_test_schem() {
        let name = "new_chest_test";
        let input_path_str = format!("tests/samples/{}.schem", name);
        let schem_path = Path::new(&input_path_str);
        assert!(schem_path.exists(), "Sample .schem file not found");
        let schem_data =
            fs::read(schem_path).unwrap_or_else(|_| panic!("Failed to read {}", input_path_str));

        let schematic = from_schematic(&schem_data).expect("Failed to parse schematic");
        assert_eq!(schematic.metadata.name, Some("Unnamed".to_string()));
    }

    #[test]
    fn test_conversion() {
        let output_dir_path = Path::new("tests/output");
        if !output_dir_path.exists() {
            fs::create_dir_all(output_dir_path)
                .expect("Failed to create output directory 'tests/output'");
        }
        let schem_name = "tests/samples/cutecounter.schem";
        let output_litematic_name = "tests/output/cutecounter.litematic";
        let output_schematic_name = "tests/output/cutecounter.schem";

        //load the schem as a UniversalSchematic
        let schem_data = fs::read(schem_name).expect("Failed to read schem file");
        let schematic = from_schematic(&schem_data).expect("Failed to parse schematic");

        //convert the UniversalSchematic to a Litematic
        let litematic_output_data =
            to_litematic(&schematic).expect("Failed to convert to litematic");
        let mut litematic_output_file =
            File::create(output_litematic_name).expect("Failed to create litematic file");
        litematic_output_file
            .write_all(&litematic_output_data)
            .expect("Failed to write litematic file");

        //load back from the litematic file
        let litematic_data =
            fs::read(output_litematic_name).expect("Failed to read litematic file");
        let schematic_from_litematic =
            from_litematic(&litematic_data).expect("Failed to parse litematic");

        //convert the Litematic back to a UniversalSchematic
        let schematic_output_data =
            to_schematic(&schematic_from_litematic).expect("Failed to convert to schematic");
        let mut schematic_output_file =
            File::create(output_schematic_name).expect("Failed to create schematic file");
        schematic_output_file
            .write_all(&schematic_output_data)
            .expect("Failed to write schematic file");
    }

    /// Test that sponge schematic export stores block entity and entity positions
    /// relative to the schematic origin [0,0,0] (without offset applied).
    #[test]
    fn test_schematic_relative_positions_in_nbt() {
        use crate::block_entity::BlockEntity;
        use crate::entity::Entity;

        let mut schematic = UniversalSchematic::new("RelativePositionTest".to_string());

        let stone = BlockState::new("minecraft:stone".to_string());
        // Place blocks at offset positions so compact region has non-zero position
        schematic.set_block(10, 20, 30, &stone);
        schematic.set_block(11, 21, 31, &stone);

        // Add a block entity at absolute position (10, 20, 30)
        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (10, 20, 30));
        schematic.default_region.add_block_entity(block_entity);

        // Add an entity at absolute position (10.5, 20.0, 30.5)
        let entity = Entity::new("minecraft:creeper".to_string(), (10.5, 20.0, 30.5));
        schematic.default_region.add_entity(entity);

        // Export to schematic v3
        let schem_data = to_schematic_v3(&schematic, DEFAULT_COMPRESSION).unwrap();

        // Parse back the raw NBT
        let reader = std::io::BufReader::new(schem_data.as_slice());
        let mut gz = GzDecoder::new(reader);
        let (root, _) = read_nbt(&mut gz, Flavor::Uncompressed).unwrap();
        let schem = root.get::<_, &NbtCompound>("Schematic").unwrap();

        // Offset should be at (10, 20, 30) - the compact region position
        let offset = schem.get::<_, &[i32]>("Offset").unwrap();
        assert_eq!(offset, &[10, 20, 30]);

        // Block entity positions should be relative to origin (not offset)
        let blocks = schem.get::<_, &NbtCompound>("Blocks").unwrap();
        let block_entities = blocks.get::<_, &NbtList>("BlockEntities").unwrap();
        assert_eq!(block_entities.len(), 1);
        if let NbtTag::Compound(be_nbt) = &block_entities[0] {
            let pos = be_nbt.get::<_, &[i32]>("Pos").unwrap();
            // Absolute (10,20,30) minus offset (10,20,30) = relative (0,0,0)
            assert_eq!(
                pos,
                &[0, 0, 0],
                "Block entity position should be relative to schematic origin, got {:?}",
                pos
            );
        } else {
            panic!("Expected compound tag for block entity");
        }

        // Test entity positions via v2 (v3 has a pre-existing bug where entities
        // are filtered out due to case-sensitive "Id" vs "id" check)
        let schem_v2_data = to_schematic_v2(&schematic, DEFAULT_COMPRESSION).unwrap();
        let reader_v2 = std::io::BufReader::new(schem_v2_data.as_slice());
        let mut gz_v2 = GzDecoder::new(reader_v2);
        let (root_v2, _) = read_nbt(&mut gz_v2, Flavor::Uncompressed).unwrap();
        let schem_v2 = root_v2.get::<_, &NbtCompound>("Schematic").unwrap();

        let entities_v2 = schem_v2.get::<_, &NbtList>("Entities").unwrap();
        assert_eq!(entities_v2.len(), 1);
        if let NbtTag::Compound(ent_nbt) = &entities_v2[0] {
            let pos = ent_nbt.get::<_, &NbtList>("Pos").unwrap();
            let x = pos.get::<f64>(0).unwrap();
            let y = pos.get::<f64>(1).unwrap();
            let z = pos.get::<f64>(2).unwrap();
            // Absolute (10.5,20.0,30.5) minus offset (10,20,30) = relative (0.5,0.0,0.5)
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

    /// Test sponge schematic roundtrip preserves positions correctly after export/import.
    #[test]
    fn test_schematic_roundtrip_with_offset_positions() {
        use crate::block_entity::BlockEntity;
        use crate::entity::Entity;

        let mut schematic = UniversalSchematic::new("OffsetRoundtrip".to_string());

        let stone = BlockState::new("minecraft:stone".to_string());
        for x in 10..13 {
            for y in 20..23 {
                for z in 30..33 {
                    schematic.set_block(x, y, z, &stone);
                }
            }
        }

        let chest = BlockEntity::new("minecraft:chest".to_string(), (10, 20, 30));
        schematic.default_region.add_block_entity(chest);

        let creeper = Entity::new("minecraft:creeper".to_string(), (11.5, 21.0, 31.5));
        schematic.default_region.add_entity(creeper);

        // Roundtrip through v2 (v3 has a pre-existing entity export filter bug)
        let schem_data = to_schematic_v2(&schematic, DEFAULT_COMPRESSION).unwrap();
        let roundtrip = from_schematic(&schem_data).unwrap();

        let rt_region = &roundtrip.default_region;

        // Block entities - since import creates region at (0,0,0), positions stay relative
        // which is correct for sponge schematic (the offset is stored separately)
        assert_eq!(rt_region.block_entities.len(), 1);
        assert!(
            rt_region.block_entities.contains_key(&(0, 0, 0)),
            "Block entity should be at relative (0,0,0) after import, keys: {:?}",
            rt_region.block_entities.keys().collect::<Vec<_>>()
        );

        // Entities - positions should be relative to schematic origin
        // Entity at absolute (11.5, 21.0, 31.5) minus offset (10, 20, 30) = relative (1.5, 1.0, 1.5)
        assert_eq!(rt_region.entities.len(), 1);
        let rt_entity = &rt_region.entities[0];
        assert!(
            (rt_entity.position.0 - 1.5).abs() < 0.001,
            "Entity X should be 1.5 relative, got {}",
            rt_entity.position.0
        );
        assert!(
            (rt_entity.position.1 - 1.0).abs() < 0.001,
            "Entity Y should be 1.0 relative, got {}",
            rt_entity.position.1
        );
        assert!(
            (rt_entity.position.2 - 1.5).abs() < 0.001,
            "Entity Z should be 1.5 relative, got {}",
            rt_entity.position.2
        );
    }
}
