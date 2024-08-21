use crate::{UniversalSchematic, Region, BlockState, Entity, BlockEntity, BoundingBox};
use quartz_nbt::{NbtCompound, NbtTag, NbtList};
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use std::io::Read;
use std::collections::HashMap;

pub fn to_schematic(schematic: &UniversalSchematic) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut root = NbtCompound::new();

    // Add Version and DataVersion fields
    root.insert("Version", NbtTag::Int(2)); // Schematic format version 2
    root.insert("DataVersion", NbtTag::Int(schematic.metadata.mc_version.unwrap_or(1343))); // Use default if not provided

    // Calculate bounding box
    let bounding_box = schematic.get_bounding_box();
    let (width, height, length) = bounding_box.get_dimensions();

    // Add width, height, length
    root.insert("Width", NbtTag::Short(width as i16));
    root.insert("Height", NbtTag::Short(height as i16));
    root.insert("Length", NbtTag::Short(length as i16));

    // Add Offset (default [0, 0, 0])
    let offset = vec![0, 0, 0];
    root.insert("Offset", NbtTag::IntArray(offset));

    // Convert and add the Palette and PaletteMax
    let mut palette = NbtCompound::new();  // Corrected: Using NbtCompound directly
    let mut max_palette_index = 0;
    for region in schematic.regions.values() {
        let (region_palette, region_max_index) = convert_palette(&region.palette);
        max_palette_index = max_palette_index.max(region_max_index);
        for (key, value) in region_palette.inner() {
            palette.insert(key.clone(), value.clone());
        }
    }
    root.insert("Palette", NbtTag::Compound(palette));  // Corrected: Now using the correct NbtCompound
    root.insert("PaletteMax", NbtTag::Int(max_palette_index + 1));

    // Convert and add BlockData
    let mut block_data = Vec::new();
    for region in schematic.regions.values() {
        block_data.extend(convert_block_data(region, &bounding_box));
    }
    root.insert("BlockData", NbtTag::ByteArray(block_data));

    // Convert and add BlockEntities
    let mut block_entities = NbtList::new();
    for region in schematic.regions.values() {
        block_entities.extend(convert_block_entities(region).iter().cloned());
    }
    root.insert("BlockEntities", NbtTag::List(block_entities));

    // Convert and add Entities
    let mut entities = NbtList::new();
    for region in schematic.regions.values() {
        entities.extend(convert_entities(region).iter().cloned());
    }
    root.insert("Entities", NbtTag::List(entities));

    // Add Metadata
    root.insert("Metadata", schematic.metadata.to_nbt()); // No need to check for `Some`, just insert the metadata directly

    // Compress and return the NBT data
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    quartz_nbt::io::write_nbt(&mut encoder, None, &root, quartz_nbt::io::Flavor::Uncompressed)?;
    Ok(encoder.finish()?)
}




pub fn from_schematic(data: &[u8]) -> Result<UniversalSchematic, Box<dyn std::error::Error>> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;

    let (root, _) = quartz_nbt::io::read_nbt(&mut std::io::Cursor::new(decompressed), quartz_nbt::io::Flavor::Uncompressed)?;

    // Correctly access the Metadata compound and the Name field
    let name = if let Some(metadata) = root.get::<_, &NbtCompound>("Metadata").ok() {
        metadata.get::<_, &str>("Name").ok().map(|s| s.to_string())
    } else {
        None
    }.unwrap_or_else(|| "Unnamed".to_string());

    let mc_version = root.get::<_, i32>("DataVersion").ok();

    let mut schematic = UniversalSchematic::new(name);
    schematic.metadata.mc_version = mc_version;

    let width = root.get::<_, i16>("Width")? as u32;
    let height = root.get::<_, i16>("Height")? as u32;
    let length = root.get::<_, i16>("Length")? as u32;

    let palette = parse_palette(&root)?;
    let block_data = parse_block_data(&root, width, height, length)?;

    let mut region = Region::new("Main".to_string(), (0, 0, 0), (width as i32, height as i32, length as i32));
    region.palette = palette;

    for (i, block_id) in block_data.iter().enumerate() {
        let x = (i % width as usize) as i32;
        let y = (i / (width * length) as usize) as i32;
        let z = ((i / width as usize) % length as usize) as i32;

        if let Some(block) = region.palette.get(*block_id as usize) {
            region.set_block(x, y, z, block.clone());
        }
    }

    let block_entities = parse_block_entities(&root)?;
    for block_entity in block_entities {
        region.add_block_entity(block_entity);
    }

    let entities = parse_entities(&root)?;
    for entity in entities {
        region.add_entity(entity);
    }

    schematic.add_region(region);
    Ok(schematic)
}


fn convert_palette(palette: &Vec<BlockState>) -> (NbtCompound, i32) {
    let mut nbt_palette = NbtCompound::new();
    let mut max_id = 0;

    for (id, block_state) in palette.iter().enumerate() {
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

fn convert_block_data(region: &Region, bounding_box: &BoundingBox) -> Vec<i8> {
    let mut block_data = Vec::new();
    let (width, height, length) = bounding_box.get_dimensions();

    for y in 0..height {
        for z in 0..length {
            for x in 0..width {
                let global_x = bounding_box.min.0 + x as i32;
                let global_y = bounding_box.min.1 + y as i32;
                let global_z = bounding_box.min.2 + z as i32;

                if let Some(block) = region.get_block(global_x, global_y, global_z) {
                    let id = region.get_palette_index(block).unwrap_or(0) as i32;
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

fn convert_block_entities(region: &Region) -> NbtList {
    let mut block_entities = NbtList::new();

    for (pos, block_entity) in &region.block_entities {
        let mut nbt = block_entity.to_nbt();
        if let NbtTag::Compound(compound) = &mut nbt {
            compound.insert("Pos", NbtTag::IntArray(vec![pos.0, pos.1, pos.2]));
        }
        block_entities.push(nbt);
    }

    block_entities
}

fn convert_entities(region: &Region) -> NbtList {
    let mut entities = NbtList::new();

    for entity in &region.entities {
        entities.push(entity.to_nbt());
    }

    entities
}

fn parse_palette(region_tag: &NbtCompound) -> Result<Vec<BlockState>, Box<dyn std::error::Error>> {
    let palette_compound = region_tag.get::<_, &NbtCompound>("Palette")?;
    let mut palette = Vec::new();

    for (key, value) in palette_compound.inner() {
        if let NbtTag::Int(_id) = value {
            let block_state = BlockState::new(key.to_string());
            palette.push(block_state);
        }
    }

    Ok(palette)
}

fn parse_block_data(region_tag: &NbtCompound, width: u32, height: u32, length: u32) -> Result<Vec<i32>, Box<dyn std::error::Error>> {
    let block_data_raw = region_tag.get::<_, &[i8]>("BlockData")?;
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

fn parse_block_entities(region_tag: &NbtCompound) -> Result<Vec<BlockEntity>, Box<dyn std::error::Error>> {
    let block_entities_list = region_tag.get::<_, &NbtList>("BlockEntities")?;
    let mut block_entities = Vec::new();

    for tag in block_entities_list.iter() {
        if let NbtTag::Compound(compound) = tag {
            block_entities.push(BlockEntity::from_nbt(compound)?);
        }
    }

    Ok(block_entities)
}

fn parse_entities(region_tag: &NbtCompound) -> Result<Vec<Entity>, Box<dyn std::error::Error>> {
    let entities_list = region_tag.get::<_, &NbtList>("Entities")?;
    let mut entities = Vec::new();

    for tag in entities_list.iter() {
        if let NbtTag::Compound(compound) = tag {
            entities.push(Entity::from_nbt(compound)?);
        }
    }

    Ok(entities)
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
        //resize the schematic
        //schematic.resize((5, 5, 5));
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
        assert_eq!(schematic.regions.len(), loaded_schematic.regions.len());

        let original_region = schematic.regions.get("Main").unwrap();
        let loaded_region = loaded_schematic.regions.get("Main").unwrap();

        assert_eq!(original_region.entities.len(), loaded_region.entities.len());
        assert_eq!(original_region.block_entities.len(), loaded_region.block_entities.len());

        // Clean up the generated file
        //std::fs::remove_file("test_schematic.schem").expect("Failed to remove file");
    }

    #[test]
    fn test_mandelbulb_generation() {
        // Create a new schematic
        let mut schematic = UniversalSchematic::new("Mandelbulb Set".to_string());

        // Define the Mandelbulb parameters
        let power = 8.0;
        let max_iterations = 10;
        let bailout = 2.0;
        let size = 128;

        // Define block states for the Mandelbulb set
        let stone = BlockState::new("minecraft:stone".to_string());
        let air = BlockState::new("minecraft:air".to_string());

        // Generate the Mandelbulb set
        for x in 0..size {
            for y in 0..size {
                for z in 0..size {
                    let x0 = (x as f64 - size as f64 / 2.0) / (size as f64 / 4.0);
                    let y0 = (y as f64 - size as f64 / 2.0) / (size as f64 / 4.0);
                    let z0 = (z as f64 - size as f64 / 2.0) / (size as f64 / 4.0);

                    let mut zx = 0.0;
                    let mut zy = 0.0;
                    let mut zz = 0.0;
                    let mut r = 0.0;
                    let mut i = 0;

                    while i < max_iterations && r < bailout {
                        let x1 = zx * zx - zy * zy - zz * zz + x0;
                        let y1 = 2.0 * zx * zy + y0;
                        let z1 = 2.0 * zx * zz + z0;

                        zx = x1;
                        zy = y1;
                        zz = z1;
                        r = (zx * zx + zy * zy + zz * zz).sqrt();
                        i += 1;
                    }

                    if r < bailout {
                        schematic.set_block(x, y, z, stone.clone());
                    } else {
                        schematic.set_block(x, y, z, air.clone());
                    }
                }
            }
        }

        // Convert the schematic to .schem format
        let schem_data = to_schematic(&schematic).expect("Failed to convert schematic");

        // Save the .schem file
        let mut file = File::create("mandelbulb.schem").expect("Failed to create file");
        file.write_all(&schem_data).expect("Failed to write to file");

        // Read the .schem file back
        let loaded_schem_data = std::fs::read("mandelbulb.schem").expect("Failed to read file");

        // Parse the loaded .schem data
        let loaded_schematic = from_schematic(&loaded_schem_data).expect("Failed to parse schematic");

        // Compare the original and loaded schematics
        assert_eq!(schematic.metadata.name, loaded_schematic.metadata.name);
        assert_eq!(schematic.regions.len(), loaded_schematic.regions.len());

        let original_region = schematic.regions.get("Main").unwrap();
        let loaded_region = loaded_schematic.regions.get("Main").unwrap();

        assert_eq!(original_region.entities.len(), loaded_region.entities.len());
        assert_eq!(original_region.block_entities.len(), loaded_region.block_entities.len());

        // Clean up the generated file
        //std::fs::remove_file("mandelbulb.schem").expect("Failed to remove file");
    }
}