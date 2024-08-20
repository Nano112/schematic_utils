use std::collections::HashMap;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq,Serialize, Deserialize)]
pub struct BlockEntity {
    pub id: String,
    pub position: (i32, i32, i32),
    pub nbt: HashMap<String, String>, // Simplified NBT data
}

impl BlockEntity {
    pub fn new(id: String, position: (i32, i32, i32)) -> Self {
        BlockEntity {
            id,
            position,
            nbt: HashMap::new(),
        }
    }

    pub fn with_nbt_data(mut self, key: String, value: String) -> Self {
        self.nbt.insert(key, value);
        self
    }

    pub fn with_item(mut self, slot: u8, item_id: &str, count: u8) -> Self {
        let item_data = format!("{{id:\"{}\",Count:{}b,Slot:{}b}}", item_id, count, slot);
        let items = self.nbt.entry("Items".to_string()).or_insert_with(|| "[]".to_string());

        if items == "[]" {
            *items = format!("[{}]", item_data);
        } else {
            *items = format!(
                "[{}{}]",
                &items[1..items.len()-1],
                format!(",{}", item_data)
            );
        }

        self
    }

    pub fn with_custom_data(mut self, key: &str, value: &str) -> Self {
        self.nbt.insert(key.to_string(), value.to_string());
        self
    }

    pub fn to_nbt(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.insert("id", NbtTag::String(self.id.clone()));

        let pos_list = NbtList::from(vec![
            NbtTag::Int(self.position.0),
            NbtTag::Int(self.position.1),
            NbtTag::Int(self.position.2)
        ]);
        compound.insert("Pos", NbtTag::List(pos_list));

        let mut nbt_data = NbtCompound::new();
        for (key, value) in &self.nbt {
            nbt_data.insert(key, NbtTag::String(value.clone()));
        }
        compound.insert("NBT", NbtTag::Compound(nbt_data));

        NbtTag::Compound(compound)
    }

    pub fn from_nbt(nbt: &NbtCompound) -> Result<Self, String> {
        let id = nbt.get::<_, &str>("id")
            .map_err(|e| format!("Failed to get BlockEntity id: {}", e))?
            .to_string();

        let position = match nbt.get::<_, &NbtTag>("Pos") {
            Ok(NbtTag::IntArray(arr)) if arr.len() == 3 => (arr[0], arr[1], arr[2]),
            Ok(NbtTag::List(list)) if list.len() == 3 => {
                (
                    list.get::<i32>(0).map_err(|e| format!("Failed to get X position: {}", e))?,
                    list.get::<i32>(1).map_err(|e| format!("Failed to get Y position: {}", e))?,
                    list.get::<i32>(2).map_err(|e| format!("Failed to get Z position: {}", e))?,
                )
            }
            _ => return Err("Invalid position data".to_string()),
        };

        let nbt_data = nbt.get::<_, &NbtCompound>("NBT")
            .map_err(|e| format!("Failed to get BlockEntity NBT data: {}", e))?;
        let mut nbt_map = HashMap::new();
        for (key, value) in nbt_data.inner() {
            if let NbtTag::String(s) = value {
                nbt_map.insert(key.clone(), s.clone());
            }
        }

        Ok(BlockEntity {
            id,
            position,
            nbt: nbt_map,
        })
    }
}