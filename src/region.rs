use std::collections::HashMap;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use crate::{ BlockState};
use crate::block_entity::BlockEntity;
use crate::block_position::BlockPosition;
use crate::bounding_box::BoundingBox;
use crate::entity::Entity;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Region {
    pub name: String,
    pub position: (i32, i32, i32),
    pub size: (i32, i32, i32),
    pub blocks: Vec<usize>,
    pub(crate) palette: Vec<BlockState>,
    pub entities: Vec<Entity>,
    #[serde(serialize_with = "serialize_block_entities", deserialize_with = "deserialize_block_entities")]
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
}

fn serialize_block_entities<S>(
    block_entities: &HashMap<(i32, i32, i32), BlockEntity>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let block_entities_vec: Vec<&BlockEntity> = block_entities.values().collect();
    block_entities_vec.serialize(serializer)
}

fn deserialize_block_entities<'de, D>(
    deserializer: D,
) -> Result<HashMap<(i32, i32, i32), BlockEntity>, D::Error>
where
    D: Deserializer<'de>,
{
    let block_entities_vec: Vec<BlockEntity> = Vec::deserialize(deserializer)?;
    Ok(block_entities_vec
        .into_iter()
        .map(|be| {
            let pos = (be.position.0 as i32, be.position.1 as i32, be.position.2 as i32);
            (pos, be)
        })
        .collect())
}
impl Region {
    pub fn new(name: String, position: (i32, i32, i32), size: (i32, i32, i32)) -> Self {
        let bounding_box = BoundingBox::from_position_and_size(position, size);
        let volume = bounding_box.volume() as usize;
        let position_and_size = bounding_box.to_position_and_size();
        let mut palette = Vec::new();
        palette.push(BlockState::new("minecraft:air".to_string()));
        Region {
            name,
            position: position_and_size.0,
            size: position_and_size.1,
            blocks: vec![0; volume],
            palette,
            entities: Vec::new(),
            block_entities: HashMap::new(),
        }
    }


    pub fn get_block_entities_as_list(&self) -> Vec<BlockEntity> {
        self.block_entities.values().cloned().collect()
    }

    pub fn is_in_region(&self, x: i32, y: i32, z: i32) -> bool {
        let bounding_box = self.get_bounding_box();
        bounding_box.contains((x, y, z))
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: BlockState) -> bool {
        if !self.is_in_region(x, y, z) {
            self.expand_to_fit(x, y, z);
        }

        let index = self.coords_to_index(x, y, z);
        let palette_index = self.get_or_insert_in_palette(block);
        self.blocks[index] = palette_index;
        true
    }

    pub fn set_block_entity(&mut self, position: BlockPosition, block_entity: BlockEntity) -> bool {
        self.block_entities.insert((position.x, position.y, position.z), block_entity);
        true
    }

    pub fn get_block_entity(&self, position: BlockPosition) -> Option<&BlockEntity> {
        self.block_entities.get(&(position.x, position.y, position.z))
    }
    pub fn get_bounding_box(&self) -> BoundingBox {
        BoundingBox::from_position_and_size(self.position, self.size)
    }

    fn coords_to_index(&self, x: i32, y: i32, z: i32) -> usize {
        self.get_bounding_box().coords_to_index(x, y, z)
    }



    pub fn index_to_coords(&self, index: usize) -> (i32, i32, i32) {
        self.get_bounding_box().index_to_coords(index)
    }

    pub fn get_dimensions(&self) -> (i32, i32, i32) {
        let bounding_box = self.get_bounding_box();
        bounding_box.get_dimensions()
    }



    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<&BlockState> {
        if !self.is_in_region(x, y, z) {
            return None;
        }

        let index = self.coords_to_index(x, y, z);
        let block_index = self.blocks[index];
        let palette_index = self.palette.get(block_index);
        palette_index
    }

    pub fn get_block_index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        if !self.is_in_region(x, y, z) {
            return None;
        }

        let index = self.coords_to_index(x, y, z);
        let block_index = self.blocks[index];
        Some(block_index)
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
        let fit_position_bounding_box = BoundingBox::new(
            (x, y, z),
            (x, y, z)
        );
        let new_bounding_box = current_bounding_box.union(&fit_position_bounding_box);
        let new_size = new_bounding_box.get_dimensions();
        let new_position = new_bounding_box.min;
        if new_size == self.size && new_position == self.position {
            return;
        }
        //get the air id
        let air_id = self.palette.iter().position(|b| b.name == "minecraft:air").unwrap();
        let mut new_blocks = vec![air_id; new_bounding_box.volume() as usize];
        for index in 0..self.blocks.len() {
            let (x, y, z) = self.index_to_coords(index);
            let new_index = new_bounding_box.coords_to_index(x, y, z);
            new_blocks[new_index] = self.blocks[index];
        }
        self.position = new_position;
        self.size = new_size;
        self.blocks = new_blocks;
    }



    fn calculate_bits_per_block(&self) -> usize {
        let palette_size = self.palette.len();
        let bits_per_block = std::cmp::max((palette_size as f64).log2().ceil() as usize, 2);
        bits_per_block
    }



    pub fn merge(&mut self, other: &Region) {
        let bounding_box = self.get_bounding_box().union(&other.get_bounding_box());
        let other_bounding_box = other.get_bounding_box();

        let combined_bounding_box = bounding_box.union(&other_bounding_box);
        let new_size = combined_bounding_box.get_dimensions();
        let new_position = combined_bounding_box.min;

        let mut new_blocks = vec![0; (combined_bounding_box.volume()) as usize];
        let mut new_palette = self.palette.clone();
        let mut reverse_new_palette: HashMap<BlockState, usize> = HashMap::new();
        for (index, block) in self.palette.iter().enumerate() {
            reverse_new_palette.insert(block.clone(), index);
        }
        for index in 0..self.blocks.len() {
            let (x, y, z) = self.index_to_coords(index);
            let new_index = ((y - new_position.1) * new_size.0 * new_size.2 + (z - new_position.2) * new_size.0 + (x - new_position.0)) as usize;
            let block_index = self.blocks[index];
            let block = &self.palette[block_index];
            if let Some(palette_index) = reverse_new_palette.get(block) {
                new_blocks[new_index] = *palette_index;
            } else {
                new_blocks[new_index] = new_palette.len();
                new_palette.push(block.clone());
                reverse_new_palette.insert(block.clone(), new_palette.len() - 1);
            }
        }

        for index in 0..other.blocks.len() {
            let (x, y, z) = other.index_to_coords(index);
            let new_index = ((y - new_position.1) * new_size.0 * new_size.2 + (z - new_position.2) * new_size.0 + (x - new_position.0)) as usize;
            let block_palette_index = other.blocks[index];
            let block = &other.palette[block_palette_index];
            if let Some(palette_index) = reverse_new_palette.get(block) {
                if block.name == "minecraft:air" {
                    continue;
                }
                new_blocks[new_index] = *palette_index;
            } else {
                new_palette.push(block.clone());
                reverse_new_palette.insert(block.clone(), new_palette.len() - 1);
                if block.name == "minecraft:air" {
                    continue;
                }
                new_blocks[new_index] = new_palette.len() - 1;

            }
        }

        // Update region properties
        self.position = new_position;
        self.size = new_size;
        self.blocks = new_blocks;
        self.palette = new_palette;


        // Merge entities and block entities
        self.merge_entities(other);
        self.merge_block_entities(other);
    }

    fn calculate_new_index(&self, x: i32, y: i32, z: i32, new_position: &(i32, i32, i32), new_size: &(i32, i32, i32)) -> usize {
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
        for (index, &block_index) in self.blocks.iter().enumerate() {
            let (x, y, z) = self.index_to_coords(index);
            blocks_tag.insert(&format!("{},{},{}", x, y, z), NbtTag::Int(block_index as i32));
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
        let mut blocks = vec![0; (size.0 * size.1 * size.2) as usize];
        for (key, value) in blocks_tag.inner() {
            if let NbtTag::Int(index) = value {
                let coords: Vec<i32> = key.split(',')
                    .map(|s| s.parse::<i32>().unwrap())
                    .collect();
                if coords.len() == 3 {
                    let block_index = (coords[1] * size.0 * size.2 + coords[2] * size.0 + coords[0]) as usize;
                    blocks[block_index] = *index as usize;
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
                    if let block_entity = BlockEntity::from_nbt(be_compound) {
                        block_entities.insert((coords[0], coords[1], coords[2]), block_entity);
                    }
                }
            }
        }

        Ok(Region {
            name,
            position,
            size,
            blocks,
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
        // TODO: Implement BlockEntity to NbtTag conversion
        region_nbt.insert("TileEntities", NbtTag::List(NbtList::new()));

        region_nbt
    }

    pub fn unpack_block_states(&self, packed_states: &[i64]) -> Vec<usize> {
        let bits_per_block = self.calculate_bits_per_block();
        let mask = (1 << bits_per_block) - 1;
        let volume = self.volume();

        let mut blocks = Vec::with_capacity(volume);

        for index in 0..volume {
            let bit_index = index * bits_per_block;
            let start_long_index = bit_index / 64;
            let start_offset = bit_index % 64;

            let value = if start_offset + bits_per_block <= 64 {
                // Block is entirely within one long
                ((packed_states[start_long_index] as u64) >> start_offset) & (mask as u64)
            } else {
                // Block spans two longs
                let low_bits = ((packed_states[start_long_index] as u64) >> start_offset) & ((1 << (64 - start_offset)) - 1);
                let high_bits = (packed_states[start_long_index + 1] as u64) & ((1 << (bits_per_block - (64 - start_offset))) - 1);
                low_bits | (high_bits << (64 - start_offset))
            };


            blocks.push(value as usize);
        }

        blocks
    }



    pub(crate) fn create_packed_block_states(&self) -> Vec<i64> {
        let bits_per_block = self.calculate_bits_per_block();
        let size = self.blocks.len();
        let expected_len = (size * bits_per_block + 63) / 64; // Equivalent to ceil(size * bits_per_block / 64)

        let mut packed_states = vec![0i64; expected_len];
        let mask = (1i64 << bits_per_block) - 1;

        for (index, &block_state) in self.blocks.iter().enumerate() {
            let bit_index = index * bits_per_block;
            let start_long_index = bit_index / 64;
            let end_long_index = (bit_index + bits_per_block - 1) / 64;
            let start_offset = bit_index % 64;

            let value = (block_state as i64) & mask;

            if start_long_index == end_long_index {
                packed_states[start_long_index] |= value << start_offset;
            } else {
                packed_states[start_long_index] |= value << start_offset;
                packed_states[end_long_index] |= value >> (64 - start_offset);
            }
        }

        // Handle negative numbers
        packed_states.iter_mut().for_each(|x| *x = *x as u64 as i64);

        packed_states
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




    pub fn count_block_types(&self) -> HashMap<BlockState, usize> {
        let mut block_counts = HashMap::new();
        for block_index in &self.blocks {
            let block_state = &self.palette[*block_index];
            *block_counts.entry(block_state.clone()).or_insert(0) += 1;
        }
        block_counts
    }

    pub fn count_blocks(&self) -> usize {
        self.blocks.iter().filter(|&&block_index| block_index != 0).count()
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
    fn test_pack_block_states_to_long_array() {
        //array from 1 to 16
        let blocks = (1..=16).collect::<Vec<usize>>();
        let mut palette = vec![BlockState::new("minecraft:air".to_string())];
        for i in 1..=16 {
            palette.push(BlockState::new(format!("minecraft:wool{}", i)));
        }
        let mut region = Region {
            name: "Test".to_string(),
            position: (0, 0, 0),
            size: (16, 1, 1),
            blocks: blocks.clone(),
            palette,
            entities: Vec::new(),
            block_entities: HashMap::new(),
        };
        let packed_states = region.create_packed_block_states();
        assert_eq!(packed_states.len(), 2);
        assert_eq!(packed_states, vec![-3013672028691362751, 33756]);

        let unpacked_blocks = region.unpack_block_states(&packed_states);
        assert_eq!(unpacked_blocks, blocks);
    }



    #[test]
    fn test_region_creation() {
        let region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        assert_eq!(region.name, "Test");
        assert_eq!(region.position, (0, 0, 0));
        assert_eq!(region.size, (2, 2, 2));
        assert_eq!(region.blocks.len(), 8);
        assert_eq!(region.palette.len(), 1);
        assert_eq!(region.palette[0].name, "minecraft:air");
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

        let mut volume1 = region.volume();
        for i in 0..8 {
            let coords = region.index_to_coords(i);
            let index = region.coords_to_index(coords.0, coords.1, coords.2);
            assert!( index < volume1);
            assert_eq!(index, i);
        }

        let region2 = Region::new("Test".to_string(), (0, 0, 0), (-2, -2, -2));

        let mut volume2 = region2.volume();
        for i in 0..8 {
            let coords = region2.index_to_coords(i);
            let index = region2.coords_to_index(coords.0, coords.1, coords.2);
            assert!(index >= 0 && index < volume2);
            assert_eq!(index, i);
        }

        region.merge(&region2);

        let mut volume3 = region.volume();
        for i in 0..27 {
            let coords = region.index_to_coords(i);
            let index = region.coords_to_index(coords.0, coords.1, coords.2);
            assert!(index >= 0 && index < volume3);
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

}