use serde::{Deserialize, Serialize};
use crate::BlockState;

pub(crate) const SECTION_SIZE: usize = 16;
const SECTION_VOLUME: usize = SECTION_SIZE * SECTION_SIZE * SECTION_SIZE;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChunkSection {
    blocks: Vec<usize>,
    palette: Vec<BlockState>,
    block_count: u32,
}

impl ChunkSection {
    pub fn new() -> Self {
        let mut palette = Vec::new();
        palette.push(BlockState::new("minecraft:air".to_string()));

        ChunkSection {
            blocks: vec![0; SECTION_VOLUME],
            palette,
            block_count: 0,
        }
    }

    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block: BlockState) -> bool {
        let index = self.get_index(x, y, z);
        let palette_index = self.get_or_insert_in_palette(block);

        let old_block_index = self.blocks[index];
        if old_block_index == 0 && palette_index != 0 {
            self.block_count += 1;
        } else if old_block_index != 0 && palette_index == 0 {
            self.block_count -= 1;
        }

        self.blocks[index] = palette_index;
        true
    }

    pub fn get_block(&self, x: usize, y: usize, z: usize) -> &BlockState {
        let index = self.get_index(x, y, z);
        let palette_index = self.blocks[index];
        &self.palette[palette_index]
    }

    fn get_index(&self, x: usize, y: usize, z: usize) -> usize {
        (y * SECTION_SIZE * SECTION_SIZE) + (z * SECTION_SIZE) + x
    }

    fn get_or_insert_in_palette(&mut self, block: BlockState) -> usize {
        if let Some(index) = self.palette.iter().position(|b| b == &block) {
            index
        } else {
            self.palette.push(block);
            self.palette.len() - 1
        }
    }

    pub fn block_count(&self) -> u32 {
        self.block_count
    }

    pub fn is_empty(&self) -> bool {
        self.block_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_chunk_section() {
        let section = ChunkSection::new();
        assert_eq!(section.block_count(), 0);
        assert!(section.is_empty());
        assert_eq!(section.palette.len(), 1);
        assert_eq!(section.palette[0].name, "minecraft:air");
    }

    #[test]
    fn test_set_and_get_block() {
        let mut section = ChunkSection::new();
        let stone = BlockState::new("minecraft:stone".to_string());

        section.set_block(0, 0, 0, stone.clone());
        assert_eq!(section.get_block(0, 0, 0), &stone);
        assert_eq!(section.block_count(), 1);

        // Check that air blocks don't increase the block count
        section.set_block(1, 1, 1, BlockState::new("minecraft:air".to_string()));
        assert_eq!(section.block_count(), 1);
    }

    #[test]
    fn test_palette_efficiency() {
        let mut section = ChunkSection::new();
        let stone = BlockState::new("minecraft:stone".to_string());

        for x in 0..SECTION_SIZE {
            for y in 0..SECTION_SIZE {
                for z in 0..SECTION_SIZE {
                    section.set_block(x, y, z, stone.clone());
                }
            }
        }

        assert_eq!(section.palette.len(), 2); // air and stone
        assert_eq!(section.block_count(), SECTION_VOLUME as u32);
    }
}