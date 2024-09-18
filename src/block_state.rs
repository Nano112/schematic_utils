use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use quartz_nbt::{NbtCompound, NbtTag};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockState {
    pub name: String,
    pub properties: HashMap<String, String>,
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.properties.is_empty() {
            write!(f, "[")?;
            for (i, (key, value)) in self.properties.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "{}={}", key, value)?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

impl Hash for BlockState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        for (k, v) in &self.properties {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl BlockState {
    pub fn new(name: String) -> Self {
        BlockState {
            name,
            properties: HashMap::new(),
        }
    }

    pub fn with_property(mut self, key: String, value: String) -> Self {
        self.properties.insert(key, value);
        self
    }

    pub fn with_properties(mut self, properties: HashMap<String, String>) -> Self {
        self.properties = properties;
        self
    }

    pub fn set_property(&mut self, key: String, value: String) {
        self.properties.insert(key, value);
    }

    pub fn remove_property(&mut self, key: &str) {
        self.properties.remove(key);
    }

    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }
    pub fn to_nbt(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.insert("Name", self.name.clone());

        if !self.properties.is_empty() {
            let mut properties = NbtCompound::new();
            for (key, value) in &self.properties {
                properties.insert(key, value.clone());
            }
            compound.insert("Properties", properties);
        }

        NbtTag::Compound(compound)
    }

    pub fn from_nbt(compound: &NbtCompound) -> Result<Self, String> {
        let name = compound
            .get::<_, &String>("Name")
            .map_err(|e| format!("Failed to get Name: {}", e))?
            .clone();

        let mut properties = HashMap::new();
        if let Ok(props) = compound.get::<_, &NbtCompound>("Properties") {
            for (key, value) in props.inner() {
                if let NbtTag::String(value_str) = value {
                    properties.insert(key.clone(), value_str.clone());
                }
            }
        }

        Ok(BlockState { name, properties })
    }





}

#[cfg(test)]
mod tests {
    use super::BlockState;

    #[test]
    fn test_block_state_creation() {
        let block = BlockState::new("minecraft:stone".to_string())
            .with_property("variant".to_string(), "granite".to_string());

        assert_eq!(block.name, "minecraft:stone");
        assert_eq!(block.properties.get("variant"), Some(&"granite".to_string()));
    }
}
