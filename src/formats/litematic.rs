use quartz_nbt::{NbtCompound, NbtTag, NbtList};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::{UniversalSchematic, BlockState};
use crate::block_entity::BlockEntity;
use crate::entity::Entity;
use crate::region::Region;

pub fn to_litematic(schematic: &UniversalSchematic) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut root = NbtCompound::new();

    // Add Version and SubVersion
    root.insert("Version", NbtTag::Int(6));
    root.insert("SubVersion", NbtTag::Int(1));

    // Add MinecraftDataVersion
    root.insert("MinecraftDataVersion", NbtTag::Int(schematic.metadata.mc_version.unwrap_or(3700)));

    // Add Metadata
    let metadata = create_metadata(schematic);
    root.insert("Metadata", NbtTag::Compound(metadata));

    // Add Regions
    let regions = create_regions(schematic);
    root.insert("Regions", NbtTag::Compound(regions));

    // Compress and return the NBT data
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    quartz_nbt::io::write_nbt(&mut encoder, None, &root, quartz_nbt::io::Flavor::Uncompressed)?;
    Ok(encoder.finish()?)
}

pub fn from_litematic(data: &[u8]) -> Result<UniversalSchematic, Box<dyn std::error::Error>> {
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut decompressed = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut decompressed)?;

    let (root, _) = quartz_nbt::io::read_nbt(&mut std::io::Cursor::new(decompressed), quartz_nbt::io::Flavor::Uncompressed)?;

    let mut schematic = UniversalSchematic::new("Unnamed".to_string());

    // Parse Metadata
    parse_metadata(&root, &mut schematic)?;

    // Parse Regions
    parse_regions(&root, &mut schematic)?;

    Ok(schematic)
}

fn create_metadata(schematic: &UniversalSchematic) -> NbtCompound {
    let mut metadata = NbtCompound::new();

    metadata.insert("Name", NbtTag::String(schematic.metadata.name.clone().unwrap_or_default()));
    metadata.insert("Description", NbtTag::String(schematic.metadata.description.clone().unwrap_or_default()));
    metadata.insert("Author", NbtTag::String(schematic.metadata.author.clone().unwrap_or_default()));

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    metadata.insert("TimeCreated", NbtTag::Long(schematic.metadata.created.unwrap_or(now as u64) as i64));
    metadata.insert("TimeModified", NbtTag::Long(schematic.metadata.modified.unwrap_or(now as u64) as i64));

    let bounding_box = schematic.get_bounding_box();
    let (width, height, length) = bounding_box.get_dimensions();
    let mut enclosing_size = NbtCompound::new();
    enclosing_size.insert("x", NbtTag::Int(width as i32));
    enclosing_size.insert("y", NbtTag::Int(height as i32));
    enclosing_size.insert("z", NbtTag::Int(length as i32));
    metadata.insert("EnclosingSize", NbtTag::Compound(enclosing_size));

    metadata.insert("TotalVolume", NbtTag::Int(schematic.total_volume() as i32));
    metadata.insert("TotalBlocks", NbtTag::Int(schematic.total_blocks() as i32));
    metadata.insert("RegionCount", NbtTag::Int(schematic.regions.len() as i32));

    metadata.insert("Software", NbtTag::String("UniversalSchematic".to_string()));

    metadata
}
fn create_regions(schematic: &UniversalSchematic) -> NbtCompound {
    let mut regions = NbtCompound::new();

    for (name, region) in &schematic.regions {
        let mut region_nbt = NbtCompound::new();

        // Position
        let mut position = NbtCompound::new();
        position.insert("x", NbtTag::Int(region.position.0));
        position.insert("y", NbtTag::Int(region.position.1));
        position.insert("z", NbtTag::Int(region.position.2));
        region_nbt.insert("Position", NbtTag::Compound(position));

        // Size
        let mut size = NbtCompound::new();
        size.insert("x", NbtTag::Int(region.size.0));
        size.insert("y", NbtTag::Int(region.size.1));
        size.insert("z", NbtTag::Int(region.size.2));
        region_nbt.insert("Size", NbtTag::Compound(size));

        // BlockStatePalette
        let palette = NbtList::from(region.palette.iter().map(|block_state| block_state.to_nbt()).collect::<Vec<NbtTag>>());
        region_nbt.insert("BlockStatePalette", NbtTag::List(palette));

        // BlockStates
        let block_states = region.create_packed_block_states();
        region_nbt.insert("BlockStates", NbtTag::LongArray(block_states));

        // Entities
        let entities = NbtList::from(region.entities.iter().map(|entity| entity.to_nbt()).collect::<Vec<NbtTag>>());
        region_nbt.insert("Entities", NbtTag::List(entities));

        // TileEntities
        let tile_entities = NbtList::from(region.block_entities.values().map(|be| be.to_nbt()).collect::<Vec<NbtTag>>());
        region_nbt.insert("TileEntities", NbtTag::List(tile_entities));

        // PendingBlockTicks and PendingFluidTicks (not fully supported, using empty lists)
        region_nbt.insert("PendingBlockTicks", NbtTag::List(NbtList::new()));
        region_nbt.insert("PendingFluidTicks", NbtTag::List(NbtList::new()));

        regions.insert(name, NbtTag::Compound(region_nbt));
    }

    regions
}


fn parse_metadata(root: &NbtCompound, schematic: &mut UniversalSchematic) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = root.get::<_, &NbtCompound>("Metadata")?;

    schematic.metadata.name = metadata.get::<_, &str>("Name").ok().map(String::from);
    schematic.metadata.description = metadata.get::<_, &str>("Description").ok().map(String::from);
    schematic.metadata.author = metadata.get::<_, &str>("Author").ok().map(String::from);
    schematic.metadata.created = metadata.get::<_, i64>("TimeCreated").ok().map(|t| t as u64);
    schematic.metadata.modified = metadata.get::<_, i64>("TimeModified").ok().map(|t| t as u64);

    // We don't need to parse EnclosingSize, TotalVolume, TotalBlocks as they will be recalculated

    Ok(())
}

fn parse_regions(root: &NbtCompound, schematic: &mut UniversalSchematic) -> Result<(), Box<dyn std::error::Error>> {
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
            region.palette = palette.iter().filter_map(|tag| {
                if let NbtTag::Compound(compound) = tag {
                    BlockState::from_nbt(compound).ok()
                } else {
                    None
                }
            }).collect();

            // Parse BlockStates
            let block_states = region_nbt.get::<_, &[i64]>("BlockStates")?;
            // region.unpack_block_states(block_states);
            region.blocks = region.unpack_block_states(block_states);
            // Parse Entities
            if let Ok(entities_list) = region_nbt.get::<_, &NbtList>("Entities") {
                region.entities = entities_list.iter().filter_map(|tag| {
                    if let NbtTag::Compound(compound) = tag {
                        Entity::from_nbt(compound).ok()
                    } else {
                        None
                    }
                }).collect();
            }

            // Parse TileEntities
            if let Ok(tile_entities_list) = region_nbt.get::<_, &NbtList>("TileEntities") {
                for tag in tile_entities_list.iter() {
                    if let NbtTag::Compound(compound) = tag {
                        if let Ok(block_entity) = BlockEntity::from_nbt(compound) {
                            region.block_entities.insert(block_entity.position, block_entity);
                        }
                    }
                }
            }

            schematic.add_region(region);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use num_complex::Complex;
    use super::*;
    use crate::{UniversalSchematic, BlockState};

    #[test]
    fn test_create_metadata() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        schematic.metadata.author = Some("Test Author".to_string());
        schematic.metadata.description = Some("Test Description".to_string());
        schematic.metadata.created = Some(1000);
        schematic.metadata.modified = Some(2000);

        let metadata = create_metadata(&schematic);

        assert_eq!(metadata.get::<_, &str>("Name").unwrap(), "Test Schematic");
        assert_eq!(metadata.get::<_, &str>("Author").unwrap(), "Test Author");
        assert_eq!(metadata.get::<_, &str>("Description").unwrap(), "Test Description");
        assert_eq!(metadata.get::<_, i64>("TimeCreated").unwrap(), 1000);
        assert_eq!(metadata.get::<_, i64>("TimeModified").unwrap(), 2000);
        assert!(metadata.contains_key("EnclosingSize"));
        assert!(metadata.contains_key("TotalVolume"));
        assert!(metadata.contains_key("TotalBlocks"));
        assert!(metadata.contains_key("RegionCount"));
        assert_eq!(metadata.get::<_, &str>("Software").unwrap(), "UniversalSchematic");
    }

    #[test]
    fn test_create_regions() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        let mut region = Region::new("TestRegion".to_string(), (0, 0, 0), (2, 2, 2));

        let stone = BlockState::new("minecraft:stone".to_string());
        let air = BlockState::new("minecraft:air".to_string());

        region.set_block(0, 0, 0, stone.clone());
        region.set_block(1, 1, 1, stone.clone());

        let entity = Entity::new("minecraft:creeper".to_string(), (0.5, 0.0, 0.5));
        region.add_entity(entity);

        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (0, 1, 0));
        region.add_block_entity(block_entity);

        schematic.add_region(region);

        let regions = create_regions(&schematic);

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
        metadata.insert("Description", NbtTag::String("Test Description".to_string()));
        metadata.insert("TimeCreated", NbtTag::Long(1000));
        metadata.insert("TimeModified", NbtTag::Long(2000));
        root.insert("Metadata", NbtTag::Compound(metadata));

        let mut schematic = UniversalSchematic::new("".to_string());
        parse_metadata(&root, &mut schematic).unwrap();

        assert_eq!(schematic.metadata.name, Some("Test Schematic".to_string()));
        assert_eq!(schematic.metadata.author, Some("Test Author".to_string()));
        assert_eq!(schematic.metadata.description, Some("Test Description".to_string()));
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

        assert_eq!(schematic.regions.len(), 1);
        assert!(schematic.regions.contains_key("TestRegion"));

        let parsed_region = schematic.regions.get("TestRegion").unwrap();
        assert_eq!(parsed_region.position, (0, 0, 0));
        assert_eq!(parsed_region.size, (2, 2, 2));
        assert_eq!(parsed_region.palette.len(), 2);
        assert_eq!(parsed_region.count_blocks(), 2); // 2 stone blocks
    }
    #[test]
    fn test_simple_litematic() {
        let mut schematic = UniversalSchematic::new("Simple Cube".to_string());

        // Create a 3x3x3 cube
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    let block = match (x + y + z) % 3 {
                        0 => BlockState::new("minecraft:stone".to_string()),
                        1 => BlockState::new("minecraft:dirt".to_string()),
                        _ => BlockState::new("minecraft:oak_planks".to_string()),
                    };
                    schematic.set_block(x, y, z, block);
                }
            }
        }

        // Set metadata
        schematic.metadata.author = Some("Test Author".to_string());
        schematic.metadata.description = Some("A simple 3x3x3 cube for testing".to_string());

        // Convert the schematic to .litematic format
        let litematic_data = to_litematic(&schematic).expect("Failed to convert schematic to litematic");

        // Save the .litematic file
        let mut file = File::create("simple_cube.litematic").expect("Failed to create file");
        file.write_all(&litematic_data).expect("Failed to write to file");

        // Read the .litematic file back
        let loaded_litematic_data = std::fs::read("simple_cube.litematic").expect("Failed to read file");


        // Clean up the generated file
        //std::fs::remove_file("simple_cube.litematic").expect("Failed to remove file");
    }

    #[test]
    fn test_litematic_roundtrip() {
        let mut original_schematic = UniversalSchematic::new("Test Schematic".to_string());
        let mut region = Region::new("TestRegion".to_string(), (0, 0, 0), (2, 2, 2));

        let stone = BlockState::new("minecraft:stone".to_string());
        let air = BlockState::new("minecraft:air".to_string());

        region.set_block(0, 0, 0, stone.clone());
        region.set_block(1, 1, 1, stone.clone());

        original_schematic.add_region(region);

        // Convert to Litematic
        let litematic_data = to_litematic(&original_schematic).unwrap();

        // Convert back from Litematic
        let roundtrip_schematic = from_litematic(&litematic_data).unwrap();

        // Compare original and roundtrip schematics
        assert_eq!(original_schematic.metadata.name, roundtrip_schematic.metadata.name);
        assert_eq!(original_schematic.regions.len(), roundtrip_schematic.regions.len());

        let original_region = original_schematic.regions.get("TestRegion").unwrap();
        let roundtrip_region = roundtrip_schematic.regions.get("TestRegion").unwrap();

        assert_eq!(original_region.position, roundtrip_region.position);
        assert_eq!(original_region.size, roundtrip_region.size);
        assert_eq!(original_region.count_blocks(), roundtrip_region.count_blocks());

        // Check if blocks are in the same positions
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    assert_eq!(original_region.get_block(x, y, z), roundtrip_region.get_block(x, y, z));
                }
            }
        }
    }

}