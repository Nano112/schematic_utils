use wasm_bindgen::prelude::*;
use serde::{Serialize, Deserialize};

mod universal_schematic;
mod region;
mod block_state;
mod entity;
mod block_entity;
mod utils;
mod formats;
mod print_utils;
mod bounding_box;
mod metadata;

// Public re-exports
pub use universal_schematic::UniversalSchematic;
pub use block_state::BlockState;
pub use formats::{litematic, schematic};
pub use print_utils::{format_schematic as print_schematic, format_json_schematic as print_json_schematic};

#[wasm_bindgen]
pub struct SchematicWrapper(UniversalSchematic);

#[wasm_bindgen]
impl SchematicWrapper {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        SchematicWrapper(UniversalSchematic::new("Default".to_string()))
    }

    pub fn from_litematic(&mut self, data: &[u8]) -> Result<(), JsValue> {
        self.0 = litematic::from_litematic(data)
            .map_err(|e| JsValue::from_str(&format!("Litematic parsing error: {}", e)))?;
        Ok(())
    }

    pub fn to_litematic(&self) -> Result<Vec<u8>, JsValue> {
        litematic::to_litematic(&self.0)
            .map_err(|e| JsValue::from_str(&format!("Litematic conversion error: {}", e)))
    }

    pub fn from_schematic(&mut self, data: &[u8]) -> Result<(), JsValue> {
        self.0 = schematic::from_schematic(data)
            .map_err(|e| JsValue::from_str(&format!("Schematic parsing error: {}", e)))?;
        Ok(())
    }

    pub fn to_schematic(&self) -> Result<Vec<u8>, JsValue> {
        schematic::to_schematic(&self.0)
            .map_err(|e| JsValue::from_str(&format!("Schematic conversion error: {}", e)))
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_name: &str) {
        self.0.set_block(x, y, z, BlockState::new(block_name.to_string()));
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<String> {
        self.0.get_block(x, y, z).map(|block_state| block_state.name.clone())
    }

    pub fn print_schematic(&self) -> String {
        print_schematic(&self.0)
    }

    pub fn debug_info(&self) -> String {
        format!("Schematic name: {}, Regions: {}",
                self.0.metadata.name.as_ref().unwrap_or(&"Unnamed".to_string()),
                self.0.regions.len()
        )
    }
}

#[wasm_bindgen]
pub fn debug_schematic(schematic: &SchematicWrapper) -> String {
    format!("{}\n{}", schematic.debug_info(), print_schematic(&schematic.0))
}

