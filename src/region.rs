use std::collections::HashMap;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeMap;
use crate::{ BlockState};
use crate::block_entity::BlockEntity;
use crate::bounding_box::BoundingBox;
use crate::chunk_section::{ChunkSection,SECTION_SIZE};
use crate::entity::Entity;
use crate::universal_schematic::BlockPosition;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Region {
    pub name: String,
    pub position: (i32, i32, i32),
    pub size: (i32, i32, i32),
    #[serde(skip)]
    pub(crate) chunks: HashMap<(i32, i32, i32), ChunkSection>,
    #[serde(skip)]
    pub(crate) palette: Vec<BlockState>,
    pub entities: Vec<Entity>,
    #[serde(serialize_with = "serialize_block_entities")]
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
}

fn serialize_block_entities<S>(
    block_entities: &HashMap<(i32, i32, i32), BlockEntity>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(block_entities.len()))?;
    for (key, value) in block_entities {
        let key_str = format!("{},{},{}", key.0, key.1, key.2);
        map.serialize_entry(&key_str, value)?;
    }
    map.end()
}

impl Region {
    pub fn new(name: String, position: (i32, i32, i32), size: (i32, i32, i32)) -> Self {
        let mut palette = Vec::new();
        palette.push(BlockState::new("minecraft:air".to_string()));
        Region {
            name,
            position,
            size,
            chunks: HashMap::new(),
            palette,
            entities: Vec::new(),
            block_entities: HashMap::new(),
        }
    }

    fn get_chunk_coords(x: i32, y: i32, z: i32) -> (i32, i32, i32) {
        (x >> 4, y >> 4, z >> 4)
    }

    fn get_local_coords(x: i32, y: i32, z: i32) -> (usize, usize, usize) {
        ((x & 15) as usize, (y & 15) as usize, (z & 15) as usize)
    }



    pub fn is_in_region(&self, x: i32, y: i32, z: i32) -> bool {
        let bounding_box = self.get_bounding_box();
        bounding_box.contains((x, y, z))
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: BlockState) -> bool {
        if !self.is_in_region(x, y, z) {
            self.expand_to_fit(x, y, z);
        }

        let (chunk_x, chunk_y, chunk_z) = Self::get_chunk_coords(x, y, z);
        let (local_x, local_y, local_z) = Self::get_local_coords(x, y, z);

        let chunk = self.chunks.entry((chunk_x, chunk_y, chunk_z)).or_insert_with(ChunkSection::new);
        chunk.set_block(local_x, local_y, local_z, block.clone());

        // Update palette
        self.get_or_insert_in_palette(block);
        true
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<&BlockState> {
        if !self.is_in_region(x, y, z) {
            return None;
        }

        let (chunk_x, chunk_y, chunk_z) = Self::get_chunk_coords(x, y, z);
        let (local_x, local_y, local_z) = Self::get_local_coords(x, y, z);

        self.chunks.get(&(chunk_x, chunk_y, chunk_z))
            .map(|chunk| chunk.get_block(local_x, local_y, local_z))
            .or_else(|| Some(&self.palette[0]))
    }

    pub fn get_bounding_box(&self) -> BoundingBox {
        BoundingBox::from_position_and_size(self.position, self.size)
    }

    pub fn coords_to_index(&self, x: i32, y: i32, z: i32) -> usize {
        self.get_bounding_box().coords_to_index(x, y, z)
            .expect("Coordinates out of bounds")
    }



    pub fn index_to_coords(&self, index: usize) -> (i32, i32, i32) {
        self.get_bounding_box().index_to_coords(index)
    }

    pub fn get_dimensions(&self) -> (i32, i32, i32) {
        let bounding_box = self.get_bounding_box();
        bounding_box.get_dimensions()
    }




    fn get_or_insert_in_palette(&mut self, block: BlockState) -> usize {
        if let Some(index) = self.palette.iter().position(|b| b == &block) {
            index
        } else {
            self.palette.push(block);
            self.palette.len() - 1
        }
    }

    pub fn volume(&self) -> usize {
        self.size.0 as usize * self.size.1 as usize * self.size.2 as usize
    }


    pub fn expand_to_fit(&mut self, x: i32, y: i32, z: i32) {
        let current_bounding_box = self.get_bounding_box();
        let fit_position_bounding_box = BoundingBox::new((x, y, z), (x, y, z));
        let new_bounding_box = current_bounding_box.union(&fit_position_bounding_box);
        let new_size = new_bounding_box.get_dimensions();
        let new_position = new_bounding_box.min;

        if new_size == self.size && new_position == self.position {
            return;
        }

        let mut new_chunks = HashMap::new();

        // Transfer existing blocks to new chunks
        for ((chunk_x, chunk_y, chunk_z), chunk) in &self.chunks {
            for local_x in 0..SECTION_SIZE {
                for local_y in 0..SECTION_SIZE {
                    for local_z in 0..SECTION_SIZE {
                        let global_x = chunk_x * SECTION_SIZE as i32 + local_x as i32;
                        let global_y = chunk_y * SECTION_SIZE as i32 + local_y as i32;
                        let global_z = chunk_z * SECTION_SIZE as i32 + local_z as i32;

                        let block = chunk.get_block(local_x, local_y, local_z);
                        if block.name != "minecraft:air" {
                            let new_chunk_x = global_x >> 4;
                            let new_chunk_y = global_y >> 4;
                            let new_chunk_z = global_z >> 4;
                            let new_local_x = (global_x & 15) as usize;
                            let new_local_y = (global_y & 15) as usize;
                            let new_local_z = (global_z & 15) as usize;

                            let new_chunk = new_chunks
                                .entry((new_chunk_x, new_chunk_y, new_chunk_z))
                                .or_insert_with(ChunkSection::new);
                            new_chunk.set_block(new_local_x, new_local_y, new_local_z, block.clone());
                        }
                    }
                }
            }
        }

        self.position = new_position;
        self.size = new_size;
        self.chunks = new_chunks;
    }




    pub fn merge(&mut self, other: &Region) {
        let bounding_box = self.get_bounding_box().union(&other.get_bounding_box());
        self.size = bounding_box.get_dimensions();
        self.position = bounding_box.min;

        let mut all_blocks_to_set = Vec::new();

        for ((chunk_x, chunk_y, chunk_z), chunk) in &other.chunks {
            for x in 0..SECTION_SIZE {
                for y in 0..SECTION_SIZE {
                    for z in 0..SECTION_SIZE {
                        let block = chunk.get_block(x, y, z);
                        if block.name != "minecraft:air" {
                            let global_x = chunk_x * SECTION_SIZE as i32 + x as i32;
                            let global_y = chunk_y * SECTION_SIZE as i32 + y as i32;
                            let global_z = chunk_z * SECTION_SIZE as i32 + z as i32;
                            all_blocks_to_set.push(((global_x, global_y, global_z), block.clone()));
                        }
                    }
                }
            }
        }

        for ((x, y, z), block) in all_blocks_to_set {
            self.set_block(x, y, z, block);
        }

        self.merge_entities(other);
        self.merge_block_entities(other);
    }


    fn _calculate_new_index(&self, x: i32, y: i32, z: i32, new_position: &(i32, i32, i32), new_size: &(i32, i32, i32)) -> usize {
        ((y - new_position.1) * new_size.0 * new_size.2 + (z - new_position.2) * new_size.0 + (x - new_position.0)) as usize
    }

    fn merge_entities(&mut self, other: &Region) {
        self.entities.extend(other.entities.iter().cloned());
    }

    fn merge_block_entities(&mut self, other: &Region) {
        self.block_entities.extend(other.block_entities.iter().map(|(&pos, be)| (pos, be.clone())));
    }
    pub fn add_entity(&mut self, entity: Entity) {
        self.entities.push(entity);
    }

    pub fn remove_entity(&mut self, index: usize) -> Option<Entity> {
        if index < self.entities.len() {
            Some(self.entities.remove(index))
        } else {
            None
        }
    }

    pub fn add_block_entity(&mut self, block_entity: BlockEntity) {
        self.block_entities.insert(block_entity.position, block_entity);
    }

    pub fn remove_block_entity(&mut self, position: (i32, i32, i32)) -> Option<BlockEntity> {
        self.block_entities.remove(&position)
    }



    pub fn to_nbt(&self) -> NbtTag {
        let mut tag = NbtCompound::new();
        tag.insert("Name", NbtTag::String(self.name.clone()));
        tag.insert("Position", NbtTag::IntArray(vec![self.position.0, self.position.1, self.position.2]));
        tag.insert("Size", NbtTag::IntArray(vec![self.size.0, self.size.1, self.size.2]));

        let mut blocks_tag = NbtCompound::new();
        for ((chunk_x, chunk_y, chunk_z), chunk) in &self.chunks {
            for x in 0..SECTION_SIZE {
                for y in 0..SECTION_SIZE {
                    for z in 0..SECTION_SIZE {
                        let global_x = chunk_x * SECTION_SIZE as i32 + x as i32;
                        let global_y = chunk_y * SECTION_SIZE as i32 + y as i32;
                        let global_z = chunk_z * SECTION_SIZE as i32 + z as i32;
                        let block = chunk.get_block(x, y, z);
                        let palette_index = self.palette.iter().position(|b| b == block).unwrap();
                        blocks_tag.insert(&format!("{},{},{}", global_x, global_y, global_z), NbtTag::Int(palette_index as i32));
                    }
                }
            }
        }
        tag.insert("Blocks", NbtTag::Compound(blocks_tag));

        let palette_list = NbtList::from(self.palette.iter().map(|b| b.to_nbt()).collect::<Vec<NbtTag>>());
        tag.insert("Palette", NbtTag::List(palette_list));

        let entities_list = NbtList::from(self.entities.iter().map(|e| e.to_nbt()).collect::<Vec<NbtTag>>());
        tag.insert("Entities", NbtTag::List(entities_list));

        let mut block_entities_tag = NbtCompound::new();
        for ((x, y, z), block_entity) in &self.block_entities {
            block_entities_tag.insert(&format!("{},{},{}", x, y, z), block_entity.to_nbt());
        }
        tag.insert("BlockEntities", NbtTag::Compound(block_entities_tag));

        NbtTag::Compound(tag)
    }

    pub fn from_nbt(nbt: &NbtCompound) -> Result<Self, String> {
        let name = nbt.get::<_, &str>("Name")
            .map_err(|e| format!("Failed to get Region Name: {}", e))?
            .to_string();

        let position = match nbt.get::<_, &NbtTag>("Position") {
            Ok(NbtTag::IntArray(arr)) if arr.len() == 3 => (arr[0], arr[1], arr[2]),
            _ => return Err("Invalid Position tag".to_string()),
        };

        let size = match nbt.get::<_, &NbtTag>("Size") {
            Ok(NbtTag::IntArray(arr)) if arr.len() == 3 => (arr[0], arr[1], arr[2]),
            _ => return Err("Invalid Size tag".to_string()),
        };

        let palette_tag = nbt.get::<_, &NbtList>("Palette")
            .map_err(|e| format!("Failed to get Palette: {}", e))?;
        let palette: Vec<BlockState> = palette_tag.iter()
            .filter_map(|tag| {
                if let NbtTag::Compound(compound) = tag {
                    BlockState::from_nbt(compound).ok()
                } else {
                    None
                }
            })
            .collect();

        let blocks_tag = nbt.get::<_, &NbtCompound>("Blocks")
            .map_err(|e| format!("Failed to get Blocks: {}", e))?;

        let mut chunks = HashMap::new();
        for (key, value) in blocks_tag.inner() {
            if let NbtTag::Int(palette_index) = value {
                let coords: Vec<i32> = key.split(',')
                    .map(|s| s.parse::<i32>().unwrap())
                    .collect();
                if coords.len() == 3 {
                    let (x, y, z) = (coords[0], coords[1], coords[2]);
                    let (chunk_x, chunk_y, chunk_z) = Region::get_chunk_coords(x, y, z);
                    let (local_x, local_y, local_z) = Region::get_local_coords(x, y, z);

                    let chunk = chunks.entry((chunk_x, chunk_y, chunk_z)).or_insert_with(ChunkSection::new);
                    let block_state = palette.get(*palette_index as usize)
                        .ok_or_else(|| format!("Invalid palette index: {}", palette_index))?;
                    chunk.set_block(local_x, local_y, local_z, block_state.clone());
                }
            }
        }

        let entities_tag = nbt.get::<_, &NbtList>("Entities")
            .map_err(|e| format!("Failed to get Entities: {}", e))?;
        let entities = entities_tag.iter()
            .filter_map(|tag| {
                if let NbtTag::Compound(compound) = tag {
                    Entity::from_nbt(compound).ok()
                } else {
                    None
                }
            })
            .collect();

        let block_entities_tag = nbt.get::<_, &NbtCompound>("BlockEntities")
            .map_err(|e| format!("Failed to get BlockEntities: {}", e))?;
        let mut block_entities = HashMap::new();
        for (key, value) in block_entities_tag.inner() {
            if let NbtTag::Compound(be_compound) = value {
                let coords: Vec<i32> = key.split(',')
                    .map(|s| s.parse::<i32>().unwrap())
                    .collect();
                if coords.len() == 3 {
                    if let Ok(block_entity) = BlockEntity::from_nbt(be_compound) {
                        block_entities.insert((coords[0], coords[1], coords[2]), block_entity);
                    }
                }
            }
        }

        Ok(Region {
            name,
            position,
            size,
            chunks,
            palette,
            entities,
            block_entities,
        })
    }
    pub fn to_litematic_nbt(&self) -> NbtCompound {
        let mut region_nbt = NbtCompound::new();

        // 1. Position and Size
        region_nbt.insert("Position", NbtTag::IntArray(vec![self.position.0, self.position.1, self.position.2]));
        region_nbt.insert("Size", NbtTag::IntArray(vec![self.size.0, self.size.1, self.size.2]));

        // 2. BlockStatePalette
        let palette_nbt = NbtList::from(self.palette.iter().map(|block_state| block_state.to_nbt()).collect::<Vec<NbtTag>>());
        region_nbt.insert("BlockStatePalette", NbtTag::List(palette_nbt));

        // 3. BlockStates (packed long array)
        let block_states = self.create_packed_block_states();
        region_nbt.insert("BlockStates", NbtTag::LongArray(block_states));

        // 4. Entities
        let entities_nbt = NbtList::from(self.entities.iter().map(|entity| entity.to_nbt()).collect::<Vec<NbtTag>>());
        region_nbt.insert("Entities", NbtTag::List(entities_nbt));

        // 5. TileEntities
        let tile_entities_nbt = NbtList::from(self.block_entities.values().map(|be| be.to_nbt()).collect::<Vec<NbtTag>>());
        region_nbt.insert("TileEntities", NbtTag::List(tile_entities_nbt));

        region_nbt
    }

    pub fn unpack_block_states(&self, packed_states: &[i64]) -> Vec<usize> {
        let bits_per_block = self.calculate_bits_per_block();
        let mask = (1u64 << bits_per_block) - 1;
        let size = self.size.0.abs() as usize * self.size.1.abs() as usize * self.size.2.abs() as usize;

        let mut blocks = Vec::with_capacity(size);

        for index in 0..size {
            let bit_index = index * bits_per_block;
            let long_index = bit_index / 64;
            let offset = bit_index % 64;

            let value = if offset + bits_per_block <= 64 {
                ((packed_states[long_index] as u64) >> offset) & mask
            } else {
                let low_bits = (packed_states[long_index] as u64) >> offset;
                let high_bits = (packed_states[long_index + 1] as u64) << (64 - offset);
                (low_bits | high_bits) & mask
            };

            blocks.push(value as usize);
        }

        blocks
    }

    pub(crate) fn calculate_bits_per_block(&self) -> usize {
        let palette_size = self.palette.len();
        if palette_size <= 1 {
            1
        } else {
            (palette_size as f64).log2().ceil() as usize
        }
    }

    pub(crate) fn create_packed_block_states(&self) -> Vec<i64> {
        let bits_per_block = self.calculate_bits_per_block();
        let size = self.size.0 as usize * self.size.1 as usize * self.size.2 as usize;
        let expected_len = (size * bits_per_block + 63) / 64;

        let mut packed_states = vec![0u64; expected_len];
        let mask = (1u64 << bits_per_block) - 1;

        for index in 0..size {
            let x = index % self.size.0 as usize;
            let y = (index / self.size.0 as usize) % self.size.1 as usize;
            let z = index / (self.size.0 as usize * self.size.1 as usize);

            let block_state = self.get_block(x as i32, y as i32, z as i32)
                .map(|b| self.palette.iter().position(|pb| pb == b).unwrap_or(0))
                .unwrap_or(0) as u64;

            let bit_index = index * bits_per_block;
            let long_index = bit_index / 64;
            let offset = bit_index % 64;

            packed_states[long_index] |= (block_state & mask) << offset;

            if offset + bits_per_block > 64 {
                packed_states[long_index + 1] |= (block_state & mask) >> (64 - offset);
            }
        }

        // Convert to i64
        packed_states.iter().map(|&x| x as i64).collect()
    }
    pub fn get_palette(&self) -> Vec<BlockState> {
        self.palette.clone()
    }
    pub(crate) fn get_palette_nbt(&self) -> NbtList {
        let mut palette = NbtList::new();
        for block in &self.palette {
            palette.push(block.to_nbt());
        }
        palette
    }

    pub fn iter_blocks(&self) -> impl Iterator<Item = (BlockPosition, &BlockState)> {
        self.chunks.iter().flat_map(move |((chunk_x, chunk_y, chunk_z), chunk)| {
            (0..SECTION_SIZE).flat_map(move |x| {
                (0..SECTION_SIZE).flat_map(move |y| {
                    (0..SECTION_SIZE).map(move |z| {
                        let global_x = chunk_x * SECTION_SIZE as i32 + x as i32;
                        let global_y = chunk_y * SECTION_SIZE as i32 + y as i32;
                        let global_z = chunk_z * SECTION_SIZE as i32 + z as i32;
                        (
                            BlockPosition { x: global_x, y: global_y, z: global_z },
                            chunk.get_block(x, y, z)
                        )
                    })
                })
            })
        })
    }




    pub fn count_block_types(&self) -> HashMap<BlockState, usize> {
        let mut block_counts = HashMap::new();

        for chunk in self.chunks.values() {
            for x in 0..SECTION_SIZE {
                for y in 0..SECTION_SIZE {
                    for z in 0..SECTION_SIZE {
                        let block_state = chunk.get_block(x, y, z);
                        *block_counts.entry(block_state.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        block_counts
    }

    pub fn count_blocks(&self) -> usize {
        let mut count = 0;
        for chunk in self.chunks.values() {
            count += chunk.block_count();
        }
        count as usize
    }

    pub fn get_palette_index(&self, block: &BlockState) -> Option<usize> {
        self.palette.iter().position(|b| b == block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockState;

    #[test]
    fn test_unpack_block_states_with_negative_size() {
        // Create a mock Region with the problematic size
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (-66, 88, 1));

        // Create a mock packed states array
        // The exact content doesn't matter for this test, we just need it to be long enough
        let packed_states = vec![0i64; 1000];

        // Call the function and check if it panics
        let result = std::panic::catch_unwind(|| {
            region.unpack_block_states(&packed_states)
        });

        // Assert that the function did not panic
        assert!(result.is_ok(), "unpack_block_states panicked with negative size");

        // If it didn't panic, let's check the output
        if let Ok(blocks) = result {
            // The expected size should be the absolute product of the dimensions
            let expected_size = 66 * 88 * 1;
            assert_eq!(blocks.len(), expected_size, "Unexpected number of blocks unpacked");

            // You might want to add more assertions here to check the content of the blocks
            // For example, check if all blocks are within the expected range:
            let max_palette_index = region.palette.len() - 1;
            for block in blocks {
                assert!(block <= max_palette_index, "Block index out of range");
            }
        }
    }

    #[test]
    fn test_simple_pack_unpack() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));

        // Add just 2 block types to the palette
        region.palette.push(BlockState::new("minecraft:block0".to_string()));
        region.palette.push(BlockState::new("minecraft:block1".to_string()));

        // Set blocks in a simple pattern
        region.set_block(0, 0, 0, region.palette[0].clone());
        region.set_block(0, 0, 1, region.palette[1].clone());
        region.set_block(0, 1, 0, region.palette[1].clone());
        region.set_block(0, 1, 1, region.palette[0].clone());
        region.set_block(1, 0, 0, region.palette[1].clone());
        region.set_block(1, 0, 1, region.palette[0].clone());
        region.set_block(1, 1, 0, region.palette[0].clone());
        region.set_block(1, 1, 1, region.palette[1].clone());

        let packed_states = region.create_packed_block_states();
        let unpacked_states = region.unpack_block_states(&packed_states);

        // Check each block individually
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    let index = region.coords_to_index(x, y, z);
                    let expected_block_index = region.get_block(x, y, z)
                        .and_then(|block| region.get_palette_index(block))
                        .unwrap_or(0);

                    assert_eq!(
                        unpacked_states[index],
                        expected_block_index,
                        "Mismatch at ({}, {}, {}): expected {}, got {}",
                        x, y, z, expected_block_index, unpacked_states[index]
                    );
                }
            }
        }
    }

    #[test]
    fn test_bits_per_block_calculation() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (1, 1, 1));
        println!("Initial palette: {:?}", region.palette);
        println!("Initial bits per block: {}", region.calculate_bits_per_block());
        assert_eq!(region.calculate_bits_per_block(), 1, "Palette size {}", region.palette.len());

        region.palette.push(BlockState::new("minecraft:block1".to_string()));
        println!("Palette after adding block1: {:?}", region.palette);
        println!("Bits per block: {}", region.calculate_bits_per_block());
        assert_eq!(region.calculate_bits_per_block(), 1, "Palette size {}", region.palette.len());

        region.palette.push(BlockState::new("minecraft:block2".to_string()));
        println!("Palette after adding block2: {:?}", region.palette);
        println!("Bits per block: {}", region.calculate_bits_per_block());
        assert_eq!(region.calculate_bits_per_block(), 2, "Palette size {}", region.palette.len());

        region.palette.push(BlockState::new("minecraft:block3".to_string()));
        println!("Palette after adding block3: {:?}", region.palette);
        println!("Bits per block: {}", region.calculate_bits_per_block());
        assert_eq!(region.calculate_bits_per_block(), 2, "Palette size {}", region.palette.len());


        for i in 4..=16 {
            region.palette.push(BlockState::new(format!("minecraft:block{}", i)));
            if i < 8 {
                assert_eq!(region.calculate_bits_per_block(), 3, "Palette size {}", region.palette.len());
            } else if i < 16 {
                assert_eq!(region.calculate_bits_per_block(), 4, "Palette size {}", region.palette.len());
            } else {
                assert_eq!(region.calculate_bits_per_block(), 5, "Palette size {}", region.palette.len());
            }
        }

        assert_eq!(region.calculate_bits_per_block(), 5);

        region.palette.push(BlockState::new("minecraft:block17".to_string()));
        assert_eq!(region.calculate_bits_per_block(), 5);

        region.palette.push(BlockState::new("minecraft:block18".to_string()));
        assert_eq!(region.calculate_bits_per_block(), 5);

    }

    #[test]
    fn test_pack_block_states_to_long_array() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (16, 1, 1));
        let mut palette = vec![BlockState::new("minecraft:air".to_string())];
        for i in 1..=16 {
            palette.push(BlockState::new(format!("minecraft:wool{}", i)));
        }
        region.palette = palette;

        // Set blocks in the chunk
        for x in 0..16 {
            region.set_block(x, 0, 0, BlockState::new(format!("minecraft:wool{}", x + 1)));
        }

        let packed_states = region.create_packed_block_states();
        assert_eq!(packed_states.len(), 2);
        assert_eq!(packed_states, vec![-3013672028691362751, 33756]);

        // We can't directly test unpacking as we removed that method, but we can verify the blocks
        for x in 0..16 {
            assert_eq!(region.get_block(x, 0, 0), Some(&BlockState::new(format!("minecraft:wool{}", x + 1))));
        }
    }

    #[test]
    fn test_region_creation() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        assert_eq!(region.name, "Test");
        assert_eq!(region.position, (0, 0, 0));
        assert_eq!(region.size, (2, 2, 2));
        assert_eq!(region.chunks.len(), 0); // No chunks created initially
        assert_eq!(region.palette.len(), 1);
        assert_eq!(region.palette[0].name, "minecraft:air");

        // Test that we can set and get a block
        let stone = BlockState::new("minecraft:stone".to_string());
        region.set_block(0, 0, 0, stone.clone());
        assert_eq!(region.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region.chunks.len(), 1); // One chunk should be created

        // Test that air blocks are returned for unset blocks
        assert_eq!(region.get_block(1, 1, 1), Some(&BlockState::new("minecraft:air".to_string())));
    }


    #[test]
    fn test_set_and_get_block() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        assert!(region.set_block(0, 0, 0, stone.clone()));
        assert_eq!(region.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region.get_block(1, 1, 1), Some(&BlockState::new("minecraft:air".to_string())));
        assert_eq!(region.get_block(2, 2, 2), None);
    }

    #[test]
    fn test_expand_to_fit() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        region.set_block(0, 0, 0, stone.clone());
        let new_size = (3, 3, 3);
        region.expand_to_fit(new_size.0, new_size.1, new_size.2);

        assert_eq!(region.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region.get_block(3, 3, 3), Some(&BlockState::new("minecraft:air".to_string())));
    }

    #[test]
    fn test_entities() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let entity = Entity::new("minecraft:creeper".to_string(), (0.5, 0.0, 0.5));

        region.add_entity(entity.clone());
        assert_eq!(region.entities.len(), 1);

        let removed = region.remove_entity(0);
        assert_eq!(removed, Some(entity));
        assert_eq!(region.entities.len(), 0);
    }

    #[test]
    fn test_block_entities() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (0, 0, 0));

        region.add_block_entity(block_entity.clone());
        assert_eq!(region.block_entities.len(), 1);

        let removed = region.remove_block_entity((0, 0, 0));
        assert_eq!(removed, Some(block_entity));
        assert_eq!(region.block_entities.len(), 0);
    }

    #[test]
    fn test_to_and_from_nbt() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());
        region.set_block(0, 0, 0, stone.clone());

        let nbt = region.to_nbt();
        let deserialized_region = match nbt {
            NbtTag::Compound(compound) => Region::from_nbt(&compound).unwrap(),
            _ => panic!("Expected NbtTag::Compound"),
        };

        assert_eq!(region.name, deserialized_region.name);
        assert_eq!(region.position, deserialized_region.position);
        assert_eq!(region.size, deserialized_region.size);
        assert_eq!(region.get_block(0, 0, 0), deserialized_region.get_block(0, 0, 0));
    }

    #[test]
    fn test_to_litematic_nbt() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());
        region.set_block(0, 0, 0, stone.clone());

        let nbt = region.to_litematic_nbt();

        assert!(nbt.contains_key("Position"));
        assert!(nbt.contains_key("Size"));
        assert!(nbt.contains_key("BlockStatePalette"));
        assert!(nbt.contains_key("BlockStates"));
        assert!(nbt.contains_key("Entities"));
        assert!(nbt.contains_key("TileEntities"));
    }

    #[test]
    fn test_count_blocks() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        assert_eq!(region.count_blocks(), 0);

        region.set_block(0, 0, 0, stone.clone());
        region.set_block(1, 1, 1, stone.clone());

        assert_eq!(region.count_blocks(), 2);
    }

    #[test]
    fn test_region_merge() {
        let mut region1 = Region::new("Test1".to_string(), (0, 0, 0), (2, 2, 2));
        let mut region2 = Region::new("Test2".to_string(), (2, 2, 2), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        region1.set_block(0, 0, 0, stone.clone());
        region2.set_block(2, 2, 2, stone.clone());

        region1.merge(&region2);

        assert_eq!(region1.size, (4, 4, 4));
        assert_eq!(region1.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region1.get_block(2, 2, 2), Some(&stone));
    }



    #[test]
    fn test_region_merge_different_palettes() {
        let mut region1 = Region::new("Test1".to_string(), (0, 0, 0), (2, 2, 2));
        let mut region2 = Region::new("Test2".to_string(), (2, 2, 2), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        region1.set_block(0, 0, 0, stone.clone());
        region2.set_block(2, 2, 2, dirt.clone());

        region1.merge(&region2);

        assert_eq!(region1.size, (4, 4, 4));
        assert_eq!(region1.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region1.get_block(2, 2, 2), Some(&dirt));
    }

    #[test]
    fn test_region_merge_different_overlapping_palettes() {
        let mut region1 = Region::new("Test1".to_string(), (0, 0, 0), (2, 2, 2));
        let mut region2 = Region::new("Test2".to_string(), (1, 1, 1), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        region1.set_block(0, 0, 0, stone.clone());
        region1.set_block(1, 1, 1, dirt.clone());

        region2.set_block(2, 2, 2, dirt.clone());

        region1.merge(&region2);

        assert_eq!(region1.size, (3, 3, 3));
        assert_eq!(region1.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region1.get_block(1, 1, 1), Some(&dirt));
        assert_eq!(region1.get_block(2, 2, 2), Some(&dirt));
    }

    #[test]
    fn test_expand_to_fit_single_block() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        // Place a block at the farthest corner to trigger resizing
        region.set_block(3, 3, 3, stone.clone());

        assert_eq!(region.position, (0, 0, 0));
        assert_eq!(region.get_block(3, 3, 3), Some(&stone));
        assert_eq!(region.get_block(0, 0, 0), Some(&BlockState::new("minecraft:air".to_string())));
    }

    #[test]
    fn test_expand_to_fit_negative_coordinates() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let dirt = BlockState::new("minecraft:dirt".to_string());

        // Place a block at a negative coordinate to trigger resizing
        region.set_block(-1, -1, -1, dirt.clone());

        assert_eq!(region.position, (-1, -1, -1)); // Expect region to shift
        assert_eq!(region.get_block(-1, -1, -1), Some(&dirt));
        assert_eq!(region.get_block(0, 0, 0), Some(&BlockState::new("minecraft:air".to_string())));
    }

    #[test]
    fn test_expand_to_fit_large_positive_coordinates() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        // Place a block far away to trigger significant resizing
        region.set_block(10, 10, 10, stone.clone());

        assert_eq!(region.position, (0, 0, 0));
        assert_eq!(region.get_block(10, 10, 10), Some(&stone));
    }

    #[test]
    fn test_expand_to_fit_corner_to_corner() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        // Place a block at one corner
        region.set_block(0, 0, 0, stone.clone());

        // Place another block far from the first to trigger resizing
        region.set_block(4, 4, 4, dirt.clone());

        assert_eq!(region.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region.get_block(4, 4, 4), Some(&dirt));
    }

    #[test]
    fn test_expand_to_fit_multiple_expansions() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        // Perform multiple expansions
        region.set_block(3, 3, 3, stone.clone());
        region.set_block(7, 7, 7, stone.clone());
        region.set_block(-2, -2, -2, stone.clone());

        assert_eq!(region.position, (-2,-2,-2));  // Position should shift
        assert_eq!(region.get_block(3, 3, 3), Some(&stone));
        assert_eq!(region.get_block(7, 7, 7), Some(&stone));
        assert_eq!(region.get_block(-2, -2, -2), Some(&stone));
    }

    #[test]
    fn test_expand_to_fit_with_existing_blocks() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (3, 3, 3));
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        // Place blocks in the initial region
        region.set_block(0, 0, 0, stone.clone());
        region.set_block(2, 2, 2, dirt.clone());

        // Trigger expansion
        region.set_block(5, 5, 5, stone.clone());

        assert_eq!(region.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region.get_block(2, 2, 2), Some(&dirt));
        assert_eq!(region.get_block(5, 5, 5), Some(&stone));
    }


    #[test]
    fn test_incremental_expansion_in_x() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        for x in 0..32 {
            region.set_block(x, 0, 0, stone.clone());
            assert_eq!(region.get_block(x, 0, 0), Some(&stone));
        }
    }

    #[test]
    fn test_incremental_expansion_in_y() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        for y in 0..32 {
            region.set_block(0, y, 0, stone.clone());
            assert_eq!(region.get_block(0, y, 0), Some(&stone));
        }
    }

    #[test]
    fn test_incremental_expansion_in_z() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        for z in 0..32 {
            region.set_block(0, 0, z, stone.clone());
            assert_eq!(region.get_block(0, 0, z), Some(&stone));
        }
    }

    #[test]
    fn test_incremental_expansion_in_x_y_z() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        for i in 0..32 {
            region.set_block(i, i, i, stone.clone());
            assert_eq!(region.get_block(i, i, i), Some(&stone));
        }
    }

    #[test]
    fn test_checkerboard_expansion() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        for x in 0..32 {
            for y in 0..32 {
                for z in 0..32 {
                    if (x + y + z) % 2 == 0 {
                        region.set_block(x, y, z, stone.clone());
                    } else {
                        region.set_block(x, y, z, dirt.clone());
                    }
                }
            }
        }

        for x in 0..32 {
            for y in 0..32 {
                for z in 0..32 {
                    let expected = if (x + y + z) % 2 == 0 {
                        &stone
                    } else {
                        &dirt
                    };
                    assert_eq!(region.get_block(x, y, z), Some(expected));
                }
            }
        }
    }


    #[test]
    fn test_bounding_box() {
        let region = Region::new("Test".to_string(), (1, 0, 1), (-2, 2, -2));
        let bounding_box = region.get_bounding_box();

        assert_eq!(bounding_box.min, (0, 0, 0));
        assert_eq!(bounding_box.max, (1, 1, 1));

        let region = Region::new("Test".to_string(), (1, 0, 1), (-3, 3, -3));
        let bounding_box = region.get_bounding_box();

        assert_eq!(bounding_box.min, (-1, 0, -1));
        assert_eq!(bounding_box.max, (1, 2, 1));
    }

    #[test]
    fn test_coords_to_index() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));

        // let  volume1 = region.volume();
        for i in 0..8 {
            let coords = region.index_to_coords(i);
            let index = region.coords_to_index(coords.0, coords.1, coords.2);
            assert_eq!(index, i);
        }

        let region2 = Region::new("Test".to_string(), (0, 0, 0), (-2, -2, -2));

        // let  volume2 = region2.volume();
        for i in 0..8 {
            let coords = region2.index_to_coords(i);
            let index = region2.coords_to_index(coords.0, coords.1, coords.2);
            assert_eq!(index, i);
        }

        region.merge(&region2);

        // let volume3 = region.volume();
        for i in 0..27 {
            let coords = region.index_to_coords(i);
            let index = region.coords_to_index(coords.0, coords.1, coords.2);
            assert_eq!(index, i);
        }



    }



    #[test]
    fn test_merge_negative_size() {
        let mut region1 = Region::new("Test1".to_string(), (0, 0, 0), (-2, -2, -2));
        let mut region2 = Region::new("Test2".to_string(), (-2, -2, -2), (-2, -2, -2));
        let stone = BlockState::new("minecraft:stone".to_string());

        region1.set_block(0, 0, 0, stone.clone());
        region2.set_block(-2, -2, -2, stone.clone());

        region1.merge(&region2);

        assert_eq!(region1.size, (4, 4, 4));
        assert_eq!(region1.get_bounding_box().min, (-3, -3, -3));
        assert_eq!(region1.get_bounding_box().max, (0, 0, 0));
        assert_eq!(region1.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region1.get_block(-2, -2, -2), Some(&stone));
    }

    #[test]
    fn test_expand_to_fit_preserve_blocks() {
        let mut region = Region::new("Test".to_string(), (1, 0, 1), (-2, 2, -2));
        let stone = BlockState::new("minecraft:stone".to_string());
        let diamond = BlockState::new("minecraft:diamond_block".to_string());

        // Set some initial blocks
        region.set_block(1, 0, 1, stone.clone());
        region.set_block(0, 1, 0, stone.clone());

        // Expand the region by setting a block outside the current bounds
        region.set_block(1, 2, 1, diamond.clone());

        // Check if the original blocks are preserved
        assert_eq!(region.get_block(1, 0, 1), Some(&stone));
        assert_eq!(region.get_block(0, 1, 0), Some(&stone));

        // Check if the new block is set correctly
        assert_eq!(region.get_block(1, 2, 1), Some(&diamond));

    }

    #[test]
    fn test_basic_pack_unpack() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        region.set_block(0, 0, 0, stone.clone());
        region.set_block(1, 1, 1, dirt.clone());

        let packed_states = region.create_packed_block_states();
        let unpacked_states = region.unpack_block_states(&packed_states);

        assert_eq!(unpacked_states[region.coords_to_index(0, 0, 0)], 1, "Stone block should have palette index 1");
        assert_eq!(unpacked_states[region.coords_to_index(1, 1, 1)], 2, "Dirt block should have palette index 2");
        assert_eq!(unpacked_states[region.coords_to_index(0, 1, 0)], 0, "Air block should have palette index 0");
    }

    #[test]
    fn test_different_palette_sizes() {
        for palette_size in 2..=17 {  // Tests 2 bits per block up to 5 bits per block
            let mut region = Region::new(format!("Test{}", palette_size), (0, 0, 0), (4, 4, 4));

            for i in 0..palette_size {
                region.palette.push(BlockState::new(format!("minecraft:block{}", i)));
            }

            for i in 0..64 {
                let x = i % 4;
                let y = (i / 4) % 4;
                let z = i / 16;
                let block_index = i % palette_size;
                region.set_block(x as i32, y as i32, z as i32, region.palette[block_index].clone());
            }

            let packed_states = region.create_packed_block_states();
            let unpacked_states = region.unpack_block_states(&packed_states);

            for i in 0..64 {
                let expected = i % palette_size;
                assert_eq!(unpacked_states[i], expected,
                           "Mismatch for palette size {}, index {}: expected {}, got {}",
                           palette_size, i, expected, unpacked_states[i]);
            }
        }
    }

    #[test]
    fn test_pack_unpack_consistency() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (16, 16, 16));

        for i in 0..10 {
            region.palette.push(BlockState::new(format!("minecraft:block{}", i)));
        }

        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    let block_index = ((x * 7 + y * 5 + z * 3) % 10) as usize;
                    region.set_block(x, y, z, region.palette[block_index].clone());
                }
            }
        }

        let packed_states = region.create_packed_block_states();
        let unpacked_states = region.unpack_block_states(&packed_states);

        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    let index = region.coords_to_index(x as i32, y as i32, z as i32);
                    let expected_block_index = ((x * 7 + y * 5 + z * 3) % 10) as usize;
                    assert_eq!(unpacked_states[index], expected_block_index,
                               "Mismatch at ({}, {}, {}): expected {}, got {}",
                               x, y, z, expected_block_index, unpacked_states[index]);
                }
            }
        }
    }

    #[test]
    fn test_coords_to_index_small_region() {
        let region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));

        assert_eq!(region.coords_to_index(0, 0, 0), 0, "Origin should be index 0");
        assert_eq!(region.coords_to_index(1, 0, 0), 1, "X+1 should be index 1");
        assert_eq!(region.coords_to_index(0, 1, 0), 2, "Y+1 should be index 2");
        assert_eq!(region.coords_to_index(0, 0, 1), 4, "Z+1 should be index 4");
        assert_eq!(region.coords_to_index(1, 1, 1), 7, "Max corner should be index 7");
    }

    #[test]
    fn test_coords_to_index_larger_region() {
        let region = Region::new("Test".to_string(), (0, 0, 0), (4, 3, 2));

        assert_eq!(region.coords_to_index(0, 0, 0), 0, "Origin should be index 0");
        assert_eq!(region.coords_to_index(3, 0, 0), 3, "X+3 should be index 3");
        assert_eq!(region.coords_to_index(0, 2, 0), 8, "Y+2 should be index 8");
        assert_eq!(region.coords_to_index(0, 0, 1), 12, "Z+1 should be index 12");
        assert_eq!(region.coords_to_index(3, 2, 1), 23, "Max corner should be index 23");
    }

    #[test]
    fn test_coords_to_index_with_offset() {
        let region = Region::new("Test".to_string(), (-1, -1, -1), (2, 2, 2));

        assert_eq!(region.coords_to_index(-1, -1, -1), 0, "Origin should be index 0");
        assert_eq!(region.coords_to_index(0, -1, -1), 1, "X+1 should be index 1");
        assert_eq!(region.coords_to_index(-1, 0, -1), 2, "Y+1 should be index 2");
        assert_eq!(region.coords_to_index(-1, -1, 0), 4, "Z+1 should be index 4");
        assert_eq!(region.coords_to_index(0, 0, 0), 7, "Max corner should be index 7");
    }

    #[test]
    #[should_panic(expected = "Coordinates out of bounds")]
    fn test_coords_to_index_out_of_bounds() {
        let region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        region.coords_to_index(2, 2, 2); // This should be out of bounds
    }

    #[test]
    fn test_index_to_coords_roundtrip() {
        let region = Region::new("Test".to_string(), (-1, -1, -1), (3, 3, 3));

        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let index = region.coords_to_index(x, y, z);
                    let (x2, y2, z2) = region.index_to_coords(index);
                    assert_eq!((x, y, z), (x2, y2, z2),
                               "Roundtrip failed for coords ({}, {}, {})", x, y, z);
                }
            }
        }
    }

    #[test]
    fn test_bits_per_block_scenarios() {
        for &palette_size in &[2usize, 3, 4, 5, 8, 16, 32, 64, 128, 256] {
            let mut region = Region::new(format!("Test{}", palette_size), (0, 0, 0), (4, 4, 4));

            for i in 0..palette_size {
                region.palette.push(BlockState::new(format!("minecraft:block{}", i)));
            }

            for i in 0..64 {
                let x = i % 4;
                let y = (i / 4) % 4;
                let z = i / 16;
                let block_index = i % palette_size;
                region.set_block(x as i32, y as i32, z as i32, region.palette[block_index].clone());
            }

            let packed_states = region.create_packed_block_states();
            let unpacked_states = region.unpack_block_states(&packed_states);

            for i in 0..64 {
                let expected = i % palette_size;
                assert_eq!(unpacked_states[i], expected,
                           "Mismatch for palette size {}, index {}: expected {}, got {}",
                           palette_size, i, expected, unpacked_states[i]);
            }
        }
    }

}