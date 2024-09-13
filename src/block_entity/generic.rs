use std::collections::HashMap;
use quartz_nbt::NbtCompound;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use crate::entity::Entity;
use crate::item::ItemStack;
use crate::utils::{NbtMap, NbtValue};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockEntity {
    pub nbt: NbtMap,
    pub id: String,
    pub position: (i32, i32, i32),
}




impl BlockEntity {
    pub fn new(id: String, position: (i32, i32, i32)) -> Self {
        BlockEntity {
            nbt: NbtMap::new(),
            id,
            position,
        }
    }

    pub fn with_nbt_data(mut self, key: String, value: NbtValue) -> Self {
        self.nbt.insert(key, value);
        self
    }

    pub fn to_hashmap(&self) -> HashMap<String, nbt::Value> {
        self.nbt.iter().map(|(key, value)| {
            (key.clone(), value.to_nbt_value())  // Use a helper function to convert NbtValue to nbt::Value
        }).collect()
    }


    pub fn add_item_stack(&mut self, item: ItemStack) {
        let mut items = self.nbt.get("Items").map(|items| {
            if let NbtValue::List(items) = items {
                items.clone()
            } else {
                vec![]
            }
        }).unwrap_or_else(|| vec![]);
        items.push(item.to_nbt());
        self.nbt.insert("Items".to_string(), NbtValue::List(items));
    }

    pub fn create_chest(position: (i32, i32, i32), items: Vec<ItemStack>) -> BlockEntity {
        let mut chest = BlockEntity::new("minecraft:chest".to_string(), position);
        for item_stack in items {
            chest.add_item_stack(item_stack);
        }
        chest
    }

    pub fn from_nbt(nbt: &NbtCompound) -> Self {
        let nbt_map = NbtMap::from_quartz_nbt(nbt);
        let id = nbt_map.get("Id")
            .and_then(|v| v.as_string())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let position = nbt_map.get("Pos")
            .and_then(|v| v.as_int_array())
            .map(|v| (v[0], v[1], v[2]))
            .unwrap_or_else(|| (0, 0, 0));
        BlockEntity { nbt: nbt_map, id, position }
    }

    pub fn to_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        for (key, value) in &self.nbt {
            nbt.insert(key, value.to_quartz_nbt());
        }
        nbt
    }
}
