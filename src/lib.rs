use wasm_bindgen::prelude::*;
use js_sys;
use js_sys::Array;

mod universal_schematic;
mod region;
mod block_state;
mod entity;
mod block_entity;
mod formats;
mod print_utils;
mod bounding_box;
mod metadata;
mod chunk_section;

// Public re-exports
pub use universal_schematic::UniversalSchematic;
pub use block_state::BlockState;
pub use formats::{litematic, schematic};
pub use print_utils::{format_schematic as print_schematic, format_json_schematic as print_json_schematic};
use web_sys::console;

#[wasm_bindgen(start)]
pub fn start() {
    console::log_1(&"Sloinay sucks".into());
}

#[wasm_bindgen]
pub struct SchematicWrapper(UniversalSchematic);

#[wasm_bindgen]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[wasm_bindgen]
impl SchematicWrapper {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console::log_1(&"SchematicWrapper created".into());
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


    // Add these methods back
    pub fn get_dimensions(&self) -> Vec<i32> {
        let (x, y, z) = self.0.get_dimensions();
        vec![x, y, z]
    }

    pub fn get_block_count(&self) -> i32 {
        self.0.total_blocks()
    }

    pub fn get_volume(&self) -> i32 {
        self.0.total_volume()
    }

    pub fn get_region_names(&self) -> Vec<String> {
        self.0.get_region_names()
    }

    pub fn blocks(&self) -> Array {
        self.0.iter_blocks()
            .map(|(pos, block)| {
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &"x".into(), &pos.x.into()).unwrap();
                js_sys::Reflect::set(&obj, &"y".into(), &pos.y.into()).unwrap();
                js_sys::Reflect::set(&obj, &"z".into(), &pos.z.into()).unwrap();
                js_sys::Reflect::set(&obj, &"name".into(), &JsValue::from_str(&block.name)).unwrap();
                let properties = js_sys::Object::new();
                for (key, value) in &block.properties {
                    js_sys::Reflect::set(&properties, &JsValue::from_str(key), &JsValue::from_str(value)).unwrap();
                }
                js_sys::Reflect::set(&obj, &"properties".into(), &properties).unwrap();
                obj
            })
            .collect::<Array>()
    }

    pub fn chunks(&self, chunk_width: i32, chunk_height: i32, chunk_length: i32) -> Array {
        self.0.iter_chunks(chunk_width, chunk_height, chunk_length)
            .map(|chunk| {
                chunk.into_iter()
                    .map(|(pos, block)| {
                        let obj = js_sys::Object::new();
                        js_sys::Reflect::set(&obj, &"x".into(), &pos.x.into()).unwrap();
                        js_sys::Reflect::set(&obj, &"y".into(), &pos.y.into()).unwrap();
                        js_sys::Reflect::set(&obj, &"z".into(), &pos.z.into()).unwrap();
                        js_sys::Reflect::set(&obj, &"name".into(), &JsValue::from_str(&block.name)).unwrap();
                        let properties = js_sys::Object::new();
                        for (key, value) in &block.properties {
                            js_sys::Reflect::set(&properties, &JsValue::from_str(key), &JsValue::from_str(value)).unwrap();
                        }
                        js_sys::Reflect::set(&obj, &"properties".into(), &properties).unwrap();
                        obj
                    })
                    .collect::<Array>()
            })
            .collect::<Array>()
    }


}

#[wasm_bindgen]
pub struct BlockStateWrapper(BlockState);

#[wasm_bindgen]
impl BlockStateWrapper {
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str) -> Self {
        BlockStateWrapper(BlockState::new(name.to_string()))
    }

    pub fn with_property(&mut self, key: &str, value: &str) {
        self.0 = self.0.clone().with_property(key.to_string(), value.to_string());
    }

    pub fn name(&self) -> String {
        self.0.name.clone()
    }

    pub fn properties(&self) -> JsValue {
        let properties = self.0.properties.clone();
        let js_properties = js_sys::Object::new();
        for (key, value) in properties {
            js_sys::Reflect::set(&js_properties, &key.into(), &value.into()).unwrap();
        }
        js_properties.into()
    }
}

#[wasm_bindgen]
pub fn debug_schematic(schematic: &SchematicWrapper) -> String {
    format!("{}\n{}", schematic.debug_info(), print_schematic(&schematic.0))
}

#[wasm_bindgen]
pub fn debug_json_schematic(schematic: &SchematicWrapper) -> String {
    format!("{}\n{}", schematic.debug_info(), print_json_schematic(&schematic.0))
}


