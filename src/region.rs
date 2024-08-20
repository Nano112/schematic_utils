use std::collections::HashMap;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeMap;
use crate::{BlockEntity, BoundingBox, Entity};

#[derive(Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    pub position: (i32, i32, i32),
    pub size: (i32, i32, i32),
    #[serde(serialize_with = "serialize_blocks")]
    pub blocks: HashMap<(i32, i32, i32), usize>, // Change from Vec to HashMap
    pub entities: Vec<Entity>,
    #[serde(serialize_with = "serialize_block_entities")]
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
}


fn serialize_blocks<S>(blocks: &HashMap<(i32, i32, i32), usize>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(blocks.len()))?;
    for (key, value) in blocks {
        let key_str = format!("{},{},{}", key.0, key.1, key.2);
        map.serialize_entry(&key_str, value)?;
    }
    map.end()
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
        let estimated_capacity = (size.0 * size.1 * size.2) as usize;

        Region {
            name,
            position,
            size,
            blocks: HashMap::with_capacity(estimated_capacity), // Pre-size the HashMap
            entities: Vec::new(),
            block_entities: HashMap::new(), // You could also pre-size this if needed
        }
    }

    pub fn index_to_coords(&self, index: usize) -> (i32, i32, i32) {
        let x = index as i32 % self.size.0;
        let y = (index as i32 / self.size.0) % self.size.1;
        let z = index as i32 / (self.size.0 * self.size.1);
        (x, y, z)

    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_index: usize) {
        self.blocks.insert((x, y, z), block_index);
    }

    pub fn volume(&self) -> usize {
        (self.size.0 * self.size.1 * self.size.2) as usize
    }

    pub fn expand_to_fit(&mut self, x: i32, y: i32, z: i32) {
        let min_x = self.position.0.min(x);
        let min_y = self.position.1.min(y);
        let min_z = self.position.2.min(z);

        let max_x = (self.position.0 + self.size.0 - 1).max(x);
        let max_y = (self.position.1 + self.size.1 - 1).max(y);
        let max_z = (self.position.2 + self.size.2 - 1).max(z);

        self.position = (min_x, min_y, min_z);
        self.size = (
            max_x - min_x + 1,
            max_y - min_y + 1,
            max_z - min_z + 1,
        );
    }

    pub fn set_block_index(&mut self, x: i32, y: i32, z: i32, block_index: usize) -> bool {
        self.blocks.insert((x, y, z), block_index);
        true
    }

    pub fn get_block_index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        self.blocks.get(&(x, y, z)).cloned()
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

    pub fn get_bounding_box(&self) -> BoundingBox {
        BoundingBox::new(
            self.position,
            (
                self.position.0 + self.size.0 - 1,
                self.position.1 + self.size.1 - 1,
                self.position.2 + self.size.2 - 1,
            ),
        )
    }

    pub fn to_nbt(&self) -> NbtTag {
        let mut tag = NbtCompound::new();
        tag.insert("Name", NbtTag::String(self.name.clone()));
        tag.insert("Position", NbtTag::IntArray(vec![self.position.0, self.position.1, self.position.2]));
        tag.insert("Size", NbtTag::IntArray(vec![self.size.0, self.size.1, self.size.2]));

        let mut blocks_tag = NbtCompound::new();
        for ((x, y, z), block_index) in &self.blocks {
            blocks_tag.insert(&format!("{},{},{}", x, y, z), NbtTag::Int(*block_index as i32));
        }
        tag.insert("Blocks", NbtTag::Compound(blocks_tag));

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

        let blocks_tag = nbt.get::<_, &NbtCompound>("Blocks")
            .map_err(|e| format!("Failed to get Blocks: {}", e))?;
        let mut blocks = HashMap::new();
        for (key, value) in blocks_tag.inner() {
            if let NbtTag::Int(index) = value {
                let coords: Vec<i32> = key.split(',')
                    .map(|s| s.parse::<i32>().unwrap())
                    .collect();
                if coords.len() == 3 {
                    blocks.insert((coords[0], coords[1], coords[2]), *index as usize);
                }
            }
        }

        let entities_tag = nbt.get::<_, &NbtTag>("Entities")
            .map_err(|e| format!("Failed to get Entities: {}", e))?;
        let entities = if let NbtTag::List(entity_list) = entities_tag {
            entity_list.iter()
                .filter_map(|tag| {
                    if let NbtTag::Compound(compound) = tag {
                        Entity::from_nbt(compound).ok()
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            return Err("Entities is not a list".to_string());
        };

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
            entities,
            block_entities,
        })
    }

}