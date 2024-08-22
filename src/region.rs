use std::collections::HashMap;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeMap;
use crate::{BlockEntity, BlockState, bounding_box, BoundingBox, Entity};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Region {
    pub name: String,
    pub position: (i32, i32, i32),
    pub size: (i32, i32, i32),
    #[serde(skip)]
    pub(crate) blocks: Vec<usize>,
    #[serde(skip)]
    pub(crate) palette: Vec<BlockState>,
    pub entities: Vec<Entity>,
    #[serde(serialize_with = "serialize_block_entities")]
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
}

const  EXPAND_FACTOR: f64 = 1.5;


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
        let volume = (size.0.abs() * size.1.abs() * size.2.abs()) as usize;
        let mut palette = Vec::new();
        palette.push(BlockState::new("minecraft:air".to_string()));

        Region {
            name,
            position,
            size,
            blocks: vec![0; volume],
            palette,
            entities: Vec::new(),
            block_entities: HashMap::new(),
        }
    }
    pub fn resize(&mut self, new_size: (i32, i32, i32)) {
        let volume = (new_size.0 * new_size.1 * new_size.2) as usize;
        //resizing needs to move the blocks to the new position
        let mut new_blocks = vec![0; volume];
        for (index, &block_index) in self.blocks.iter().enumerate() {
            let (x, y, z) = self.index_to_coords(index);
            if x < new_size.0 && y < new_size.1 && z < new_size.2 {
                let new_index = (y * new_size.0 * new_size.2 + z * new_size.0 + x) as usize;
                new_blocks[new_index] = block_index;
            }
        }
        self.size = new_size;
        self.blocks = new_blocks;
    }

    pub fn is_in_region(&self, x: i32, y: i32, z: i32) -> bool {
        let in_x = if self.size.0 >= 0 {
            x >= self.position.0 && x < self.position.0 + self.size.0
        } else {
            x > self.position.0 + self.size.0 && x <= self.position.0
        };

        let in_y = if self.size.1 >= 0 {
            y >= self.position.1 && y < self.position.1 + self.size.1
        } else {
            y > self.position.1 + self.size.1 && y <= self.position.1
        };

        let in_z = if self.size.2 >= 0 {
            z >= self.position.2 && z < self.position.2 + self.size.2
        } else {
            z > self.position.2 + self.size.2 && z <= self.position.2
        };

        in_x && in_y && in_z
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

    pub fn get_bounding_box(&self) -> BoundingBox {
        let end = (
            self.position.0 + self.size.0 - self.size.0.signum(),
            self.position.1 + self.size.1 - self.size.1.signum(),
            self.position.2 + self.size.2 - self.size.2.signum()
        );

        BoundingBox {
            min: (
                self.position.0.min(end.0),
                self.position.1.min(end.1),
                self.position.2.min(end.2)
            ),
            max: (
                self.position.0.max(end.0),
                self.position.1.max(end.1),
                self.position.2.max(end.2)
            )
        }
    }

    fn coords_to_index(&self, x: i32, y: i32, z: i32) -> usize {
        let bounding_box= self.get_bounding_box();
        let dx = x - bounding_box.min.0;
        let dy = y - bounding_box.min.1;
        let dz = z - bounding_box.min.2;
        (dy * self.size.0.abs() * self.size.2.abs() + dz * self.size.0.abs() + dx) as usize
    }



    pub fn index_to_coords(&self, index: usize) -> (i32, i32, i32) {
        let bounding_box = self.get_bounding_box();
        //first compute the absolute position within the bounding box, and then add the min position
        let dimensions = bounding_box.get_dimensions();
        let x = index as i32 % dimensions.0 + bounding_box.min.0;
        let y = (index as i32 / (dimensions.0 * dimensions.2)) % dimensions.1 + bounding_box.min.1;
        let z = (index as i32 / dimensions.0) % dimensions.2 + bounding_box.min.2;
        (x, y, z)

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
        (self.size.0.abs() * self.size.1.abs() * self.size.2.abs()) as usize
    }

    const EXPAND_FACTOR: f64 = 1.5;

    pub fn expand_to_fit(&mut self, x: i32, y: i32, z: i32) {
        let min_x = self.position.0.min(x);
        let min_y = self.position.1.min(y);
        let min_z = self.position.2.min(z);

        let max_x = (self.position.0 + self.size.0 - 1).max(x);
        let max_y = (self.position.1 + self.size.1 - 1).max(y);
        let max_z = (self.position.2 + self.size.2 - 1).max(z);

        let required_size = (
            (max_x - min_x + 1) as f64,
            (max_y - min_y + 1) as f64,
            (max_z - min_z + 1) as f64,
        );

        let new_size = (
            ((required_size.0 * EXPAND_FACTOR).ceil() as i32).max(self.size.0),
            ((required_size.1 * EXPAND_FACTOR).ceil() as i32).max(self.size.1),
            ((required_size.2 * EXPAND_FACTOR).ceil() as i32).max(self.size.2),
        );

        if new_size == self.size && min_x == self.position.0 && min_y == self.position.1 && min_z == self.position.2 {
            return; // No need to expand
        }

        let mut new_blocks = vec![0; (new_size.0 * new_size.1 * new_size.2) as usize];

        // Calculate the offset for existing blocks in the new array
        let offset_x = self.position.0 - min_x;
        let offset_y = self.position.1 - min_y;
        let offset_z = self.position.2 - min_z;

        // Copy existing blocks to their new positions
        for x in 0..self.size.0 {
            for y in 0..self.size.1 {
                for z in 0..self.size.2 {
                    let old_index = (y * self.size.0 * self.size.2 + z * self.size.0 + x) as usize;
                    let new_x = x + offset_x;
                    let new_y = y + offset_y;
                    let new_z = z + offset_z;
                    let new_index = (new_y * new_size.0 * new_size.2 + new_z * new_size.0 + new_x) as usize;
                    new_blocks[new_index] = self.blocks[old_index];
                }
            }
        }

        // Update region properties
        self.position = (min_x, min_y, min_z);
        self.size = new_size;
        self.blocks = new_blocks;
    }


    pub(crate) fn unpack_block_states(&mut self, packed_states: &[i64]) {
        let bits_per_block = (self.palette.len() as f64).log2().ceil() as usize;
        let blocks_per_long = 64 / bits_per_block;
        let mask = (1 << bits_per_block) - 1;

        self.blocks.clear();

        for &long in packed_states {
            for i in 0..blocks_per_long {
                let block_id = (long >> (i * bits_per_block)) & mask;
                self.blocks.push(block_id as usize);

                if self.blocks.len() == self.volume() {
                    return;
                }
            }
        }
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
        let tile_entities_nbt = NbtList::from(self.block_entities.values().map(|be| be.to_nbt()).collect::<Vec<NbtTag>>());
        region_nbt.insert("TileEntities", NbtTag::List(tile_entities_nbt));

        region_nbt
    }

    pub(crate) fn create_packed_block_states(&self) -> Vec<i64> {
        let bits_per_block = (self.palette.len() as f64).log2().ceil() as usize;
        let blocks_per_long = 64 / bits_per_block;
        let mask = (1 << bits_per_block) - 1;

        let mut packed_states = Vec::new();
        let mut current_long = 0i64;
        let mut blocks_in_current_long = 0;

        for block_id in &self.blocks {
            current_long |= (*block_id as i64 & mask) << (blocks_in_current_long * bits_per_block);
            blocks_in_current_long += 1;

            if blocks_in_current_long == blocks_per_long {
                packed_states.push(current_long);
                current_long = 0;
                blocks_in_current_long = 0;
            }
        }

        if blocks_in_current_long > 0 {
            packed_states.push(current_long);
        }

        packed_states
    }

    pub(crate) fn palette(&self) -> NbtList {
        let mut palette = NbtList::new();
        for block in &self.palette {
            palette.push(block.to_nbt());
        }
        palette
    }

    fn calculate_bits_per_block(&self) -> usize {
        let palette_size = self.palette.len();
        std::cmp::max((palette_size as f64).log2().ceil() as usize, 2)
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

        assert_eq!(region.size, (6, 6, 6));
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

        assert_eq!(region.size, (6, 6, 6));
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
        assert_eq!(region.size, (5, 5, 5));
        assert_eq!(region.get_block(-1, -1, -1), Some(&dirt));
        assert_eq!(region.get_block(0, 0, 0), Some(&BlockState::new("minecraft:air".to_string())));
    }

    #[test]
    fn test_expand_to_fit_large_positive_coordinates() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));
        let stone = BlockState::new("minecraft:stone".to_string());

        // Place a block far away to trigger significant resizing
        region.set_block(10, 10, 10, stone.clone());

        assert_eq!(region.size, (17, 17, 17));
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

        assert_eq!(region.size, (8,8,8));  // Check the size after expansion
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
        assert_eq!(region.size, (21, 21, 21));  // Size should account for all expansions
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

        assert_eq!(region.size, (9,9,9));  // New size after expansion
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
            assert!(index >= 0 && index < volume1);
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

}