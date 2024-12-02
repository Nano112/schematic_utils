// src/block_position.rs

// Core functionality that's needed everywhere
use serde::{Serialize, Deserialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

// Core implementation that's available to all users of the library
impl BlockPosition {
    // Basic constructor that will work in any context
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        BlockPosition { x, y, z }
    }

    // Utility methods for coordinate manipulation
    pub fn to_tuple(&self) -> (i32, i32, i32) {
        (self.x, self.y, self.z)
    }

    pub fn from_tuple(tuple: (i32, i32, i32)) -> Self {
        BlockPosition::new(tuple.0, tuple.1, tuple.2)
    }
}

// WASM-specific functionality in a separate module
#[cfg(feature = "wasm")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    // Expose BlockPosition to JavaScript/WebAssembly
    #[wasm_bindgen]
    impl BlockPosition {
        // Constructor specifically for WASM contexts
        #[wasm_bindgen(constructor)]
        pub fn wasm_new(x: i32, y: i32, z: i32) -> Self {
            // We can reuse the core constructor
            Self::new(x, y, z)
        }
    }
}

// Re-export WASM functionality when the feature is enabled
#[cfg(feature = "wasm")]
pub use wasm::*;