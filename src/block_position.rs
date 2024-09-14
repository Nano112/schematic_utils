use wasm_bindgen::prelude::*;
use serde::{Serialize, Deserialize};

#[wasm_bindgen]
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[wasm_bindgen]
impl BlockPosition {
    #[wasm_bindgen(constructor)]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        BlockPosition { x, y, z }
    }
}

impl BlockPosition {
    pub fn to_tuple(&self) -> (i32, i32, i32) {
        (self.x, self.y, self.z)
    }

    pub fn from_tuple(tuple: (i32, i32, i32)) -> Self {
        BlockPosition::new(tuple.0, tuple.1, tuple.2)
    }
}