use serde::{Serialize, Deserialize};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

// WASM-specific implementation
#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl BlockPosition {
    #[wasm_bindgen(constructor)]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        BlockPosition { x, y, z }
    }
}

// Core implementation available to all users of the library
impl BlockPosition {

    #[cfg(not(feature = "wasm"))]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        BlockPosition { x, y, z }
    }

    pub fn to_tuple(&self) -> (i32, i32, i32) {
        (self.x, self.y, self.z)
    }

    pub fn from_tuple(tuple: (i32, i32, i32)) -> Self {
        BlockPosition::new(tuple.0, tuple.1, tuple.2)
    }
}