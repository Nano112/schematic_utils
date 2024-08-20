use std::collections::HashMap;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq,Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub position: (f64, f64, f64),
    pub nbt: HashMap<String, String>, // Simplified NBT data
}

impl Entity {
    pub fn new(id: String, position: (f64, f64, f64)) -> Self {
        Entity {
            id,
            position,
            nbt: HashMap::new(),
        }
    }

    pub fn with_nbt_data(mut self, key: String, value: String) -> Self {
        self.nbt.insert(key, value);
        self
    }

    pub fn to_nbt(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.insert("id", NbtTag::String(self.id.clone()));

        let pos_list = NbtList::from(vec![
            NbtTag::Double(self.position.0),
            NbtTag::Double(self.position.1),
            NbtTag::Double(self.position.2)
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
            .map_err(|e| format!("Failed to get Entity id: {}", e))?
            .to_string();

        let position = nbt.get::<_, &NbtList>("Pos")
            .map_err(|e| format!("Failed to get Entity position: {}", e))?;
        let position = if position.len() == 3 {
            (
                position.get::<f64>(0).map_err(|e| format!("Failed to get X position: {}", e))?,
                position.get::<f64>(1).map_err(|e| format!("Failed to get Y position: {}", e))?,
                position.get::<f64>(2).map_err(|e| format!("Failed to get Z position: {}", e))?,
            )
        } else {
            return Err("Invalid position data".to_string());
        };

        let nbt_data = nbt.get::<_, &NbtCompound>("NBT")
            .map_err(|e| format!("Failed to get Entity NBT data: {}", e))?;
        let mut nbt_map = HashMap::new();
        for (key, value) in nbt_data.inner() {
            if let NbtTag::String(s) = value {
                nbt_map.insert(key.clone(), s.clone());
            }
        }

        Ok(Entity {
            id,
            position,
            nbt: nbt_map,
        })
    }
}