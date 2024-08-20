use crate::{UniversalSchematic, Region, BlockState, Entity, BlockEntity, BoundingBox};
use quartz_nbt::{NbtCompound, NbtTag, NbtList};
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use std::io::Read;
use std::collections::HashMap;

pub fn to_schematic(schematic: &UniversalSchematic) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut root = NbtCompound::new();

    // Add required fields
    root.insert("Version", NbtTag::Int(2));
    root.insert("DataVersion", NbtTag::Int(2975)); // Minecraft 1.19.4 data version

    // Add metadata
    let mut metadata = NbtCompound::new();
    if let Some(name) = &schematic.metadata.name {
        metadata.insert("Name", NbtTag::String(name.clone()));
    }
    if let Some(author) = &schematic.metadata.author {
        metadata.insert("Author", NbtTag::String(author.clone()));
    } else {
        // Default value or skip insertion if author is None
        metadata.insert("Author", NbtTag::String("UniversalSchematic".to_string()));
    }
    root.insert("Metadata", NbtTag::Compound(metadata));

    // Calculate dimensions
    let bounding_box = schematic.get_bounding_box();
    let (width, height, length) = bounding_box.get_dimensions();
    root.insert("Width", NbtTag::Short(width as i16));
    root.insert("Height", NbtTag::Short(height as i16));
    root.insert("Length", NbtTag::Short(length as i16));

    // Convert palette
    let mut palette = schematic.palette.clone();
    let block_data = convert_block_data(schematic, &bounding_box, &mut palette);

    let (nbt_palette, palette_max) = convert_palette(&palette);
    root.insert("Palette", NbtTag::Compound(nbt_palette));
    root.insert("PaletteMax", NbtTag::Int(palette_max));

    root.insert("BlockData", NbtTag::ByteArray(block_data));

    // Convert block entities
    let block_entities = convert_block_entities(schematic);
    root.insert("BlockEntities", NbtTag::List(block_entities));

    // Convert entities
    let entities = convert_entities(schematic);
    root.insert("Entities", NbtTag::List(entities));

    // Serialize to NBT
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    quartz_nbt::io::write_nbt(&mut encoder, None, &root, quartz_nbt::io::Flavor::Uncompressed)?;
    Ok(encoder.finish()?)
}


pub fn from_schematic(data: &[u8]) -> Result<UniversalSchematic, Box<dyn std::error::Error>> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;

    let (root, _) = quartz_nbt::io::read_nbt(&mut std::io::Cursor::new(decompressed), quartz_nbt::io::Flavor::Uncompressed)?;

    let name = root.get::<_, &NbtCompound>("Metadata")
        .and_then(|metadata| metadata.get::<_, &str>("Name"))
        .map_or_else(|_| "Imported Schematic".to_string(), |s| s.to_string());

    let width = root.get::<_, i16>("Width")? as u32;
    let height = root.get::<_, i16>("Height")? as u32;
    let length = root.get::<_, i16>("Length")? as u32;

    let palette = parse_palette(&root)?;
    let block_data = parse_block_data(&root, width, height, length)?;
    let block_entities = parse_block_entities(&root)?;
    let entities = parse_entities(&root)?;

    let mut schematic = UniversalSchematic::new(name);
    let region = Region::new("Main".to_string(), (0, 0, 0), (width as i32, height as i32, length as i32));
    schematic.regions.insert("Main".to_string(), region);

    // Populate the schematic with blocks, block entities, and entities
    populate_schematic(&mut schematic, &palette, block_data, block_entities, entities, width, height, length)?;

    Ok(schematic)
}

fn convert_palette(palette: &crate::GlobalPalette) -> (NbtCompound, i32) {
    let mut nbt_palette = NbtCompound::new();
    let mut max_id = 0;

    for (id, block_state) in palette.blocks.iter().enumerate() {
        let key = if block_state.properties.is_empty() {
            block_state.name.clone()
        } else {
            format!("{}[{}]", block_state.name,
                    block_state.properties.iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>()
                        .join(","))
        };
        nbt_palette.insert(&key, NbtTag::Int(id as i32));
        max_id = max_id.max(id);
    }

    (nbt_palette, max_id as i32)
}

fn convert_block_data(schematic: &UniversalSchematic, bounding_box: &BoundingBox, palette: &mut crate::GlobalPalette) -> Vec<i8> {
    let mut block_data = Vec::new();
    let (width, height, length) = bounding_box.get_dimensions();

    for y in 0..height {
        for z in 0..length {
            for x in 0..width {
                let global_x = bounding_box.min.0 + x as i32;
                let global_y = bounding_box.min.1 + y as i32;
                let global_z = bounding_box.min.2 + z as i32;

                if let Some(block) = schematic.get_block(global_x, global_y, global_z) {
                    let id = palette.get_or_insert(block.clone()) as i32;
                    // Convert id to VarInt
                    let mut varint = id as u32;
                    loop {
                        let mut byte = (varint & 0x7F) as u8;
                        varint >>= 7;
                        if varint != 0 {
                            byte |= 0x80;
                        }
                        block_data.push(byte as i8);
                        if varint == 0 {
                            break;
                        }
                    }
                } else {
                    // If no block is found, use air (id 0)
                    block_data.push(0);
                }
            }
        }
    }

    block_data
}


fn convert_block_entities(schematic: &UniversalSchematic) -> NbtList {
    let mut block_entities = NbtList::new();

    for region in schematic.regions.values() {
        for (pos, block_entity) in &region.block_entities {
            let mut nbt = block_entity.to_nbt();
            if let NbtTag::Compound(compound) = &mut nbt {
                compound.insert("Pos", NbtTag::IntArray(vec![pos.0, pos.1, pos.2]));
            }
            block_entities.push(nbt);
        }
    }

    block_entities
}

fn convert_entities(schematic: &UniversalSchematic) -> NbtList {
    let mut entities = NbtList::new();

    for region in schematic.regions.values() {
        for entity in &region.entities {
            entities.push(entity.to_nbt());
        }
    }

    entities
}

fn parse_palette(root: &NbtCompound) -> Result<HashMap<i32, BlockState>, Box<dyn std::error::Error>> {
    let palette_compound = root.get::<_, &NbtCompound>("Palette")?;
    let mut palette = HashMap::new();

    for (key, value) in palette_compound.inner() {
        if let NbtTag::Int(id) = value {
            let block_state = BlockState::new(key.to_string());
            palette.insert(*id, block_state);
        }
    }

    Ok(palette)
}

fn parse_block_data(root: &NbtCompound, width: u32, height: u32, length: u32) -> Result<Vec<i32>, Box<dyn std::error::Error>> {
    let block_data_raw = root.get::<_, &[i8]>("BlockData")?;
    let mut block_data = Vec::new();
    let mut index = 0;

    while index < block_data_raw.len() {
        let mut value = 0;
        let mut size = 0;

        loop {
            let byte = block_data_raw[index] as u32;
            value |= (byte & 0x7F) << (size * 7);
            size += 1;
            index += 1;

            if byte & 0x80 == 0 {
                break;
            }
        }

        block_data.push(value as i32);
    }

    if block_data.len() != (width * height * length) as usize {
        return Err("Invalid block data length".into());
    }

    Ok(block_data)
}

fn parse_block_entities(root: &NbtCompound) -> Result<Vec<BlockEntity>, Box<dyn std::error::Error>> {
    let block_entities_list = root.get::<_, &NbtList>("BlockEntities")?;
    let mut block_entities = Vec::new();

    for tag in block_entities_list.iter() {
        if let NbtTag::Compound(compound) = tag {
            block_entities.push(BlockEntity::from_nbt(compound)?);
        }
    }

    Ok(block_entities)
}

fn parse_entities(root: &NbtCompound) -> Result<Vec<Entity>, Box<dyn std::error::Error>> {
    let entities_list = root.get::<_, &NbtList>("Entities")?;
    let mut entities = Vec::new();

    for tag in entities_list.iter() {
        if let NbtTag::Compound(compound) = tag {
            entities.push(Entity::from_nbt(compound)?);
        }
    }

    Ok(entities)
}

fn populate_schematic(
    schematic: &mut UniversalSchematic,
    palette: &HashMap<i32, BlockState>,
    block_data: Vec<i32>,
    block_entities: Vec<BlockEntity>,
    entities: Vec<Entity>,
    width: u32,
    height: u32,
    length: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut index = 0;
    for y in 0..height {
        for z in 0..length {
            for x in 0..width {
                let block_id = block_data[index];
                if let Some(block_state) = palette.get(&block_id) {
                    schematic.set_block(x as i32, y as i32, z as i32, block_state.clone());
                }
                index += 1;
            }
        }
    }

    for block_entity in block_entities {
        schematic.add_block_entity(block_entity);
    }

    for entity in entities {
        schematic.add_entity(entity);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use super::*;
    use crate::{UniversalSchematic, BlockState};

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
                        schematic.set_block(x, y, z, stone.clone());
                    } else {
                        schematic.set_block(x, y, z, dirt.clone());
                    }
                }
            }
        }


        // Convert the schematic to .schem format
        let schem_data = to_schematic(&schematic).expect("Failed to convert schematic");

        // Save the .schem file
        let mut file = File::create("test_schematic.schem").expect("Failed to create file");
        file.write_all(&schem_data).expect("Failed to write to file");

        // Read the .schem file back
        let loaded_schem_data = std::fs::read("test_schematic.schem").expect("Failed to read file");

        // Parse the loaded .schem data
        let loaded_schematic = from_schematic(&loaded_schem_data).expect("Failed to parse schematic");

        // Compare the original and loaded schematics
        assert_eq!(schematic.metadata.name, loaded_schematic.metadata.name);
        assert_eq!(schematic.palette.len(), loaded_schematic.palette.len());
        assert_eq!(schematic.regions.len(), loaded_schematic.regions.len());

        let original_region = schematic.regions.get("Main").unwrap();
        let loaded_region = loaded_schematic.regions.get("Main").unwrap();

        assert_eq!(original_region.entities.len(), loaded_region.entities.len());
        assert_eq!(original_region.block_entities.len(), loaded_region.block_entities.len());

        // Clean up the generated file
        //std::fs::remove_file("test_schematic.schem").expect("Failed to remove file");
    }

    #[test]
    fn test_convert_palette() {
        let mut palette = crate::GlobalPalette::new();
        palette.get_or_insert(BlockState::new("minecraft:stone".to_string()));
        palette.get_or_insert(BlockState::new("minecraft:dirt".to_string()));

        let (nbt_palette, max_id) = convert_palette(&palette);

        assert_eq!(max_id, 2);
        assert_eq!(nbt_palette.len(), 3); // Including air
        assert!(nbt_palette.get::<_, i32>("minecraft:stone").is_ok());
        assert!(nbt_palette.get::<_, i32>("minecraft:dirt").is_ok());
    }

    #[test]
    fn test_convert_block_data() {
        let mut schematic = UniversalSchematic::new("Test".to_string());
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        schematic.set_block(0, 0, 0, stone.clone());
        schematic.set_block(1, 0, 0, dirt.clone());

        let bounding_box = BoundingBox::new((0, 0, 0), (1, 0, 0));
        let block_data = convert_block_data(&schematic, &bounding_box, &mut crate::GlobalPalette::new());

        assert_eq!(block_data.len(), 2);
        assert_eq!(block_data[0], 1); // stone
        assert_eq!(block_data[1], 2); // dirt
    }

    #[test]
    fn test_parse_palette() {
        let mut nbt_palette = NbtCompound::new();
        nbt_palette.insert("minecraft:stone", NbtTag::Int(1));
        nbt_palette.insert("minecraft:dirt", NbtTag::Int(2));

        let mut root = NbtCompound::new();
        root.insert("Palette", NbtTag::Compound(nbt_palette));

        let palette = parse_palette(&root).unwrap();

        assert_eq!(palette.len(), 2);
        assert_eq!(palette[&1].name, "minecraft:stone");
        assert_eq!(palette[&2].name, "minecraft:dirt");
    }

    #[test]
    fn test_parse_block_data() {
        let block_data = vec![1i8, 2, 3, 4];
        let mut root = NbtCompound::new();
        root.insert("BlockData", NbtTag::ByteArray(block_data));

        let parsed_data = parse_block_data(&root, 2, 1, 2).unwrap();

        assert_eq!(parsed_data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_populate_schematic() {
        let mut schematic = UniversalSchematic::new("Test".to_string());
        let mut palette = HashMap::new();
        palette.insert(1, BlockState::new("minecraft:stone".to_string()));
        palette.insert(2, BlockState::new("minecraft:dirt".to_string()));

        let block_data = vec![1, 2, 1, 2];
        let block_entities = vec![];
        let entities = vec![];

        populate_schematic(&mut schematic, &palette, block_data, block_entities, entities, 2, 1, 2).unwrap();

        assert_eq!(schematic.get_block(0, 0, 0).unwrap().name, "minecraft:stone");
        assert_eq!(schematic.get_block(1, 0, 0).unwrap().name, "minecraft:dirt");
        assert_eq!(schematic.get_block(0, 0, 1).unwrap().name, "minecraft:stone");
        assert_eq!(schematic.get_block(1, 0, 1).unwrap().name, "minecraft:dirt");
    }
}