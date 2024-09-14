use std::collections::HashMap;
use ::mchprs_world::World;
use wasm_bindgen::prelude::*;
use js_sys;
use js_sys::{Array, Object, Reflect};
use mchprs_blocks::BlockPos;
use mchprs_blocks::blocks::Block;
use mchprs_redpiler::Compiler;

mod universal_schematic;
mod region;
mod block_state;
mod entity;
pub mod block_entity;
mod formats;
mod print_utils;
mod bounding_box;
mod metadata;
mod mchprs_world;
mod block_position;
pub mod utils;
mod item;
mod chunk;

// Public re-exports
pub use universal_schematic::UniversalSchematic;
pub use block_state::BlockState;
pub use formats::{litematic, schematic};
pub use print_utils::{format_schematic as print_schematic, format_json_schematic as print_json_schematic};
use web_sys::console;
use crate::block_position::BlockPosition;
use crate::mchprs_world::MchprsWorld;

#[wasm_bindgen(start)]
pub fn start() {
    console::log_1(&"Sloinay sucks".into());
}

#[wasm_bindgen]
pub struct SchematicWrapper(UniversalSchematic);


#[wasm_bindgen]
pub struct MchprsWorldWrapper {
    world: MchprsWorld,
    compiler: Option<Compiler>,
}

#[wasm_bindgen]
impl MchprsWorldWrapper {

    #[wasm_bindgen]
    pub fn run_simulation_step(&mut self) {

    }

    #[wasm_bindgen]
    pub fn get_updated_blocks(&mut self) -> js_sys::Array {
        let updated_blocks = js_sys::Array::new();
        let changed_positions = self.world.take_changed_blocks();

        for pos in changed_positions {
            let block_id = self.world.get_block_raw(pos);
            let block = Block::from_id(block_id);
            let block_name = block.get_name();
            let block_properties = block.properties();

            // Create JavaScript object for the block
            let block_js = js_sys::Object::new();
            js_sys::Reflect::set(&block_js, &"x".into(), &JsValue::from(pos.x)).unwrap();
            js_sys::Reflect::set(&block_js, &"y".into(), &JsValue::from(pos.y)).unwrap();
            js_sys::Reflect::set(&block_js, &"z".into(), &JsValue::from(pos.z)).unwrap();
            js_sys::Reflect::set(&block_js, &"name".into(), &JsValue::from_str(block_name)).unwrap();

            // Add properties
            let properties_js = js_sys::Object::new();
            for (key, value) in block_properties {
                js_sys::Reflect::set(&properties_js, &key.into(), &value.into()).unwrap();
            }
            js_sys::Reflect::set(&block_js, &"properties".into(), &properties_js).unwrap();

            updated_blocks.push(&block_js);
        }

        updated_blocks
    }

    #[wasm_bindgen]
    pub fn on_use_block(&mut self, x: i32, y: i32, z: i32) {
        let pos = BlockPos::new(x, y, z);
        if let Some(compiler) = &mut self.compiler {
            compiler.on_use_block(pos);
        }
    }
}

#[wasm_bindgen]
impl SchematicWrapper {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console::log_1(&"SchematicWrapper created".into());
        SchematicWrapper(UniversalSchematic::new("Default".to_string()))
    }


    pub fn from_data(&mut self, data: &[u8]) -> Result<(), JsValue> {
        if litematic::is_litematic(data) {
            self.from_litematic(data)
        } else if schematic::is_schematic(data) {
            self.from_schematic(data)
        } else {
            Err(JsValue::from_str("Unknown or unsupported schematic format"))
        }
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

    #[wasm_bindgen]
    pub fn create_simulation_world(&self) -> MchprsWorldWrapper {
        let world = MchprsWorld::new(self.0.clone());
        MchprsWorldWrapper {
            world,
            compiler: None,
        }
    }

    pub fn set_block_with_properties(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block_name: &str,
        properties: &JsValue,
    ) -> Result<(), JsValue> {
        // Convert JsValue to HashMap<String, String>
        let mut props = HashMap::new();

        if !properties.is_undefined() && !properties.is_null() {
            let obj: Object = properties.clone().dyn_into().map_err(|_| {
                JsValue::from_str("Properties should be an object")
            })?;

            let keys = js_sys::Object::keys(&obj);
            for i in 0..keys.length() {
                let key = keys.get(i);
                let key_str = key.as_string().ok_or_else(|| {
                    JsValue::from_str("Property keys should be strings")
                })?;

                let value = Reflect::get(&obj, &key).map_err(|_| {
                    JsValue::from_str("Error getting property value")
                })?;

                let value_str = value.as_string().ok_or_else(|| {
                    JsValue::from_str("Property values should be strings")
                })?;

                props.insert(key_str, value_str);
            }
        }

        // Create BlockState with properties
        let block_state = BlockState {
            name: block_name.to_string(),
            properties: props,
        };

        // Set the block in the schematic
        self.0.set_block(x, y, z, block_state);

        Ok(())
    }


    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<String> {
        self.0.get_block(x, y, z).map(|block_state| block_state.name.clone())
    }

    pub fn get_block_with_properties(&self, x: i32, y: i32, z: i32) -> Option<BlockStateWrapper> {
        self.0.get_block(x, y, z).cloned().map(BlockStateWrapper)
    }

    pub fn get_block_entity(&self, x: i32, y: i32, z: i32) -> JsValue {
        let block_position = BlockPosition { x, y, z };
        if let Some(block_entity) = self.0.get_block_entity(block_position) {
            if block_entity.id.contains("chest") {
                let obj = Object::new();
                Reflect::set(&obj, &"id".into(), &JsValue::from_str(&block_entity.id)).unwrap();

                let position = Array::new();
                position.push(&JsValue::from(block_entity.position.0));
                position.push(&JsValue::from(block_entity.position.1));
                position.push(&JsValue::from(block_entity.position.2));
                Reflect::set(&obj, &"position".into(), &position).unwrap();

                // Use the new to_js_value method
                Reflect::set(&obj, &"nbt".into(), &block_entity.nbt.to_js_value()).unwrap();

                obj.into()
            } else {
                JsValue::NULL
            }
        } else {
            JsValue::NULL
        }
    }

    pub fn get_all_block_entities(&self) -> JsValue {
        let block_entities = self.0.get_block_entities_as_list();
        let js_block_entities = Array::new();
        for block_entity in block_entities {
            let obj = Object::new();
            Reflect::set(&obj, &"id".into(), &JsValue::from_str(&block_entity.id)).unwrap();

            let position = Array::new();
            position.push(&JsValue::from(block_entity.position.0));
            position.push(&JsValue::from(block_entity.position.1));
            position.push(&JsValue::from(block_entity.position.2));
            Reflect::set(&obj, &"position".into(), &position).unwrap();

            // Use the new to_js_value method
            Reflect::set(&obj, &"nbt".into(), &block_entity.nbt.to_js_value()).unwrap();

            js_block_entities.push(&obj);
        }
        js_block_entities.into()
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
                let chunk_obj = js_sys::Object::new();
                js_sys::Reflect::set(&chunk_obj, &"chunk_x".into(), &chunk.chunk_x.into()).unwrap();
                js_sys::Reflect::set(&chunk_obj, &"chunk_y".into(), &chunk.chunk_y.into()).unwrap();
                js_sys::Reflect::set(&chunk_obj, &"chunk_z".into(), &chunk.chunk_z.into()).unwrap();

                let blocks_array = chunk.positions.into_iter()
                    .map(|pos| {
                        let block = self.0.get_block(pos.x, pos.y, pos.z).unwrap();
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
                    .collect::<Array>();

                js_sys::Reflect::set(&chunk_obj, &"blocks".into(), &blocks_array).unwrap();
                chunk_obj
            })
            .collect::<Array>()
    }


    pub fn get_chunk_blocks(&self, offset_x: i32, offset_y: i32, offset_z: i32, width: i32, height: i32, length: i32) -> js_sys::Array {
        let blocks = self.0.iter_blocks()
            .filter(|(pos, _)| {
                pos.x >= offset_x && pos.x < offset_x + width &&
                    pos.y >= offset_y && pos.y < offset_y + height &&
                    pos.z >= offset_z && pos.z < offset_z + length
            })
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
            .collect::<js_sys::Array>();

        blocks
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


