// use std::collections::HashMap;
// use std::error::Error;
// use std::io::{Cursor, Read, Write};
// use log::debug;
// use quartz_nbt::{NbtCompound, NbtList, NbtTag};
// use quartz_nbt::io::Flavor;
// use crate::{UniversalSchematic, Region, BlockState, Entity, BlockEntity, BoundingBox, GlobalPalette};
//
// pub fn to_litematic(schematic: &UniversalSchematic) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
//     let mut root = NbtCompound::new();
//
//     // Add metadata
//     let mut metadata = NbtCompound::new();
//     metadata.insert("Name", NbtTag::String(schematic.name.clone()));
//     metadata.insert("Author", NbtTag::String(schematic.author.clone()));
//     metadata.insert("Description", NbtTag::String(schematic.description.clone()));
//     metadata.insert("RegionCount", NbtTag::Int(schematic.regions.len() as i32));
//     metadata.insert("TimeCreated", NbtTag::Long(schematic.created as i64));
//     metadata.insert("TimeModified", NbtTag::Long(schematic.modified as i64));
//
//     let total_blocks: i32 = schematic.regions.values().map(|r| r.count_blocks() as i32).sum();
//     let total_volume: i32 = schematic.regions.values().map(|r| r.volume() as i32).sum();
//     metadata.insert("TotalBlocks", NbtTag::Int(total_blocks));
//     metadata.insert("TotalVolume", NbtTag::Int(total_volume));
//
//     let bounding_box = schematic.get_bounding_box();
//     let (width, height, length) = bounding_box.get_dimensions();
//     let mut enclosing_size = NbtCompound::new();
//     enclosing_size.insert("x", NbtTag::Int(width as i32));
//     enclosing_size.insert("y", NbtTag::Int(height as i32));
//     enclosing_size.insert("z", NbtTag::Int(length as i32));
//     metadata.insert("EnclosingSize", NbtTag::Compound(enclosing_size));
//
//     root.insert("Metadata", NbtTag::Compound(metadata));
//
//     // Add version information
//     root.insert("Version", NbtTag::Int(schematic.lm_version));
//     root.insert("MinecraftDataVersion", NbtTag::Int(schematic.mc_version));
//
//     // Convert regions
//     let mut regions = NbtCompound::new();
//     for (name, region) in &schematic.regions {
//         regions.insert(name, region_to_nbt(region, &schematic.palette)?);
//     }
//     root.insert("Regions", NbtTag::Compound(regions));
//
//     // Serialize to NBT
//     let mut output = Vec::new();
//     quartz_nbt::io::write_nbt(&mut output, None, &root, Flavor::Uncompressed)?;
//     Ok(output)
// }
//
// pub fn from_litematic(data: &[u8]) -> Result<UniversalSchematic, Box<dyn std::error::Error>> {
//     let (root, _) = quartz_nbt::io::read_nbt(&mut Cursor::new(data), Flavor::Uncompressed)?;
//
//     let metadata = root.get::<_, &NbtCompound>("Metadata")?;
//     let name = metadata.get::<_, &str>("Name")?.to_string();
//     let author = metadata.get::<_, &str>("Author")?.to_string();
//     let description = metadata.get::<_, &str>("Description")?.to_string();
//     let created = metadata.get::<_, i64>("TimeCreated")? as u64;
//     let modified = metadata.get::<_, i64>("TimeModified")? as u64;
//
//     let lm_version = root.get::<_, i32>("Version")?;
//     let mc_version = root.get::<_, i32>("MinecraftDataVersion")?;
//
//     let regions_nbt = root.get::<_, &NbtCompound>("Regions")?;
//     let mut regions = HashMap::new();
//     let mut global_palette = GlobalPalette::new();
//
//     for (name, region_nbt) in regions_nbt.iter() {
//         if let NbtTag::Compound(region_compound) = region_nbt {
//             let region = region_from_nbt(region_compound, &mut global_palette)?;
//             regions.insert(name.to_string(), region);
//         }
//     }
//
//     let mut schematic = UniversalSchematic::new(name);
//     schematic.author = author;
//     schematic.description = description;
//     schematic.created = created;
//     schematic.modified = modified;
//     schematic.lm_version = lm_version;
//     schematic.mc_version = mc_version;
//     schematic.regions = regions;
//     schematic.palette = global_palette;
//
//     Ok(schematic)
// }
//
// fn region_to_nbt(region: &Region, global_palette: &GlobalPalette) -> Result<NbtTag, Box<dyn std::error::Error>> {
//     let mut root = NbtCompound::new();
//
//     // Position
//     let mut position = NbtCompound::new();
//     position.insert("x", NbtTag::Int(region.x));
//     position.insert("y", NbtTag::Int(region.y));
//     position.insert("z", NbtTag::Int(region.z));
//     root.insert("Position", NbtTag::Compound(position));
//
//     // Size
//     let mut size = NbtCompound::new();
//     size.insert("x", NbtTag::Int(region.size.0));
//     size.insert("y", NbtTag::Int(region.size.1));
//     size.insert("z", NbtTag::Int(region.size.2));
//     root.insert("Size", NbtTag::Compound(size));
//
//     // Create local palette and remap block states
//     let (local_palette, remapped_blocks) = create_local_palette(region, global_palette)?;
//
//     // BlockStatePalette
//     let palette_nbt: NbtList = local_palette.iter().map(|state| state.to_nbt()).collect();
//     root.insert("BlockStatePalette", NbtTag::List(palette_nbt));
//
//     // BlockStates
//     let block_states = encode_block_states(&remapped_blocks, local_palette.len())?;
//     root.insert("BlockStates", NbtTag::LongArray(block_states));
//
//     // Entities
//     let entities: NbtList = region.entities.iter().map(|entity| entity.to_nbt()).collect();
//     root.insert("Entities", NbtTag::List(entities));
//
//     // TileEntities
//     let tile_entities: NbtList = region.block_entities.iter().map(|be| be.to_nbt()).collect();
//     root.insert("TileEntities", NbtTag::List(tile_entities));
//
//     Ok(NbtTag::Compound(root))
// }
//
// fn region_from_nbt(nbt: &NbtCompound, global_palette: &mut GlobalPalette) -> Result<Region, Box<dyn std::error::Error>> {
//     let name = nbt.get::<_, &str>("Name")?.to_string();
//     let position = nbt.get::<_, &NbtCompound>("Position")?;
//     let size = nbt.get::<_, &NbtCompound>("Size")?;
//
//     let x = position.get::<_, i32>("x")?;
//     let y = position.get::<_, i32>("y")?;
//     let z = position.get::<_, i32>("z")?;
//     let width = size.get::<_, i32>("x")?;
//     let height = size.get::<_, i32>("y")?;
//     let length = size.get::<_, i32>("z")?;
//
//     let mut region = Region::new(name, (x, y, z), (width, height, length));
//
//     // BlockStatePalette
//     let palette = nbt.get::<_, &NbtList>("BlockStatePalette")?;
//     let local_palette: Vec<BlockState> = palette.iter()
//         .map(|state| BlockState::from_nbt(state))
//         .collect::<Result<Vec<_>, _>>()?;
//
//     // BlockStates
//     let block_states = nbt.get::<_, &[i64]>("BlockStates")?;
//     let decoded_blocks = decode_block_states(block_states, local_palette.len(), region.volume())?;
//
//     // Set blocks using global palette
//     for (index, &local_index) in decoded_blocks.iter().enumerate() {
//         let block_state = local_palette[local_index].clone();
//         let global_index = global_palette.get_or_insert(block_state);
//         let (x, y, z) = region.index_to_coords(index);
//         region.set_block(x, y, z, global_index);
//     }
//
//     // Entities
//     if let Ok(entities) = nbt.get::<_, &NbtList>("Entities") {
//         region.entities = entities.iter().map(|e| Entity::from_nbt(e)).collect::<Result<Vec<_>, _>>()?;
//     }
//
//     // TileEntities
//     if let Ok(tile_entities) = nbt.get::<_, &NbtList>("TileEntities") {
//         region.block_entities = tile_entities.iter().map(|te| BlockEntity::from_nbt(te)).collect::<Result<Vec<_>, _>>()?;
//     }
//
//     Ok(region)
// }
//
// fn create_local_palette(region: &Region, global_palette: &GlobalPalette) -> Result<(Vec<BlockState>, Vec<usize>), Box<dyn std::error::Error>> {
//     let mut local_palette = Vec::new();
//     let mut global_to_local = HashMap::new();
//     let mut remapped_blocks = Vec::with_capacity(region.volume());
//
//     for (x, y, z) in region.iter_coords() {
//         let global_index = region.get_block(x, y, z);
//         let global_block = global_palette.get(global_index).ok_or("Invalid block index in global palette")?;
//         let local_index = if let Some(&index) = global_to_local.get(&global_index) {
//             index
//         } else {
//             let new_index = local_palette.len();
//             local_palette.push(global_block.clone());
//             global_to_local.insert(global_index, new_index);
//             new_index
//         };
//         remapped_blocks.push(local_index);
//     }
//
//     Ok((local_palette, remapped_blocks))
// }
//
// fn encode_block_states(blocks: &[usize], palette_size: usize) -> Result<Vec<i64>, Box<dyn std::error::Error>> {
//     let bits_per_block = (palette_size as f64).log2().ceil() as usize;
//     let blocks_per_long = 64 / bits_per_block;
//     let mask = (1 << bits_per_block) - 1;
//
//     let mut encoded = Vec::new();
//     let mut current_long = 0i64;
//     let mut blocks_in_current_long = 0;
//
//     for &block_index in blocks {
//         current_long |= ((block_index as i64) & mask) << (bits_per_block * blocks_in_current_long);
//         blocks_in_current_long += 1;
//
//         if blocks_in_current_long == blocks_per_long {
//             encoded.push(current_long);
//             current_long = 0;
//             blocks_in_current_long = 0;
//         }
//     }
//
//     if blocks_in_current_long > 0 {
//         encoded.push(current_long);
//     }
//
//     Ok(encoded)
// }
//
// fn decode_block_states(block_states: &[i64], palette_size: usize, volume: usize) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
//     let bits_per_block = (palette_size as f64).log2().ceil() as usize;
//     let blocks_per_long = 64 / bits_per_block;
//     let mask = (1 << bits_per_block) - 1;
//
//     let mut decoded = Vec::with_capacity(volume);
//
//     for &long in block_states {
//         for i in 0..blocks_per_long {
//             if decoded.len() >= volume {
//                 break;
//             }
//             let palette_index = ((long >> (i * bits_per_block)) & mask) as usize;
//             decoded.push(palette_index);
//         }
//     }
//
//     if decoded.len() != volume {
//         return Err("Mismatch between decoded block count and expected volume".into());
//     }
//
//     Ok(decoded)
// }