use crate::block_position::BlockPosition;

pub struct Chunk {
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub chunk_z: i32,
    pub positions: Vec<BlockPosition>,
}