use std::collections::HashMap;
use std::fmt;
use std::fmt::Write;
use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use crate::BlockState;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GlobalPalette {
    pub(crate) blocks: Vec<BlockState>,
    #[serde(serialize_with = "serialize_block_to_index", deserialize_with = "deserialize_block_to_index")]
    block_to_index: HashMap<BlockState, usize>,
}

// Custom serialization for block_to_index
fn serialize_block_to_index<S>(
    block_to_index: &HashMap<BlockState, usize>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(block_to_index.len()))?;
    for (block, index) in block_to_index {
        let key = serde_json::to_string(block).map_err(serde::ser::Error::custom)?;
        map.serialize_entry(&key, index)?;
    }
    map.end()
}

// Custom deserialization for block_to_index
fn deserialize_block_to_index<'de, D>(
    deserializer: D,
) -> Result<HashMap<BlockState, usize>, D::Error>
where
    D: Deserializer<'de>,
{
    struct BlockToIndexVisitor;

    impl<'de> Visitor<'de> for BlockToIndexVisitor {
        type Value = HashMap<BlockState, usize>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map with BlockState keys and usize values")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut block_to_index = HashMap::with_capacity(map.size_hint().unwrap_or(0));
            while let Some((key, value)) = map.next_entry::<String, usize>()? {
                let block: BlockState = serde_json::from_str(&key).map_err(de::Error::custom)?;
                block_to_index.insert(block, value);
            }
            Ok(block_to_index)
        }
    }

    deserializer.deserialize_map(BlockToIndexVisitor)
}


impl GlobalPalette {
    pub fn new() -> Self {
        let air = BlockState::new("minecraft:air".to_string());
        let blocks = vec![air.clone()];
        let mut block_to_index = HashMap::new();
        block_to_index.insert(air, 0);
        GlobalPalette {
            blocks,
            block_to_index,
        }
    }

    pub fn get_or_insert(&mut self, block: BlockState) -> usize {
        if let Some(&index) = self.block_to_index.get(&block) {
            index
        } else {
            let index = self.blocks.len();
            self.blocks.push(block.clone());
            self.block_to_index.insert(block, index);
            index
        }
    }

    pub fn get(&self, index: usize) -> Option<&BlockState> {
        self.blocks.get(index)
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn to_nbt(&self) -> NbtTag {
        let mut palette_tag = NbtCompound::new();

        let blocks_list = NbtList::from(self.blocks.iter().map(|b| NbtTag::String(b.name.clone())).collect::<Vec<NbtTag>>());
        palette_tag.insert("Blocks", NbtTag::List(blocks_list));

        let mut block_to_index_tag = NbtCompound::new();
        for (block, index) in &self.block_to_index {
            block_to_index_tag.insert(&block.name, NbtTag::Int(*index as i32));
        }
        palette_tag.insert("BlockToIndex", NbtTag::Compound(block_to_index_tag));

        NbtTag::Compound(palette_tag)
    }

    pub fn from_nbt(nbt: &NbtCompound) -> Result<Self, String> {
        let blocks_tag = nbt.get::<_, &NbtTag>("Blocks")
            .map_err(|e| format!("Failed to get Blocks: {}", e))?;
        let blocks = if let NbtTag::List(block_list) = blocks_tag {
            block_list.iter()
                .filter_map(|tag| {
                    if let NbtTag::String(name) = tag {
                        Some(BlockState::new(name.clone()))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            return Err("Blocks is not a list".to_string());
        };

        let block_to_index_tag = nbt.get::<_, &NbtCompound>("BlockToIndex")
            .map_err(|e| format!("Failed to get BlockToIndex: {}", e))?;
        let mut block_to_index = HashMap::new();
        for (name, index_tag) in block_to_index_tag.inner() {
            if let NbtTag::Int(index) = index_tag {
                block_to_index.insert(BlockState::new(name.clone()), *index as usize);
            }
        }

        Ok(GlobalPalette {
            blocks,
            block_to_index,
        })
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_operations() {
        let mut palette = GlobalPalette::new();

        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        assert_eq!(palette.get_or_insert(stone.clone()), 1);
        assert_eq!(palette.get_or_insert(dirt.clone()), 2);
        assert_eq!(palette.get_or_insert(stone.clone()), 1);

        assert_eq!(palette.get(0), Some(&BlockState::new("minecraft:air".to_string())));
        assert_eq!(palette.get(1), Some(&stone));
        assert_eq!(palette.get(2), Some(&dirt));
        assert_eq!(palette.get(3), None);

        assert_eq!(palette.len(), 3);
    }
}