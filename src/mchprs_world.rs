use std::collections::HashMap;
use mchprs_blocks::{block_entities::BlockEntity, blocks::Block, BlockPos};
use mchprs_world::{storage::Chunk, TickEntry, TickPriority, World};
use mchprs_redpiler::{Compiler, CompilerOptions};
use nbt::{Map, Value};
use crate::block_entity::BlockEntity as UtilBlockEntity;
use crate::UniversalSchematic;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MchprsWorldError {
    #[error("Initialization failed: {0}")]
    InitializationFailed(String),
    // Add other error variants as needed
}

pub struct MchprsWorld {
    schematic: UniversalSchematic,
    chunks: HashMap<(i32, i32), Chunk>,
    to_be_ticked: Vec<TickEntry>,
    compiler: Compiler,
}

impl MchprsWorld {
    pub fn new(schematic: UniversalSchematic) -> Self {
        let mut world = MchprsWorld {
            schematic,
            chunks: HashMap::new(),
            to_be_ticked: Vec::new(),
            compiler: Compiler::default(),
        };

        world.initialize_chunks().map_err(|e| {
            MchprsWorldError::InitializationFailed(format!("initialize_chunks failed: {}", e))
        }).expect("TODO: panic message");

        world.populate_chunks();
        world.update_redstone();
        world.initialize_compiler();
        world
    }

    fn initialize_compiler(&mut self) {
        let bounding_box = self.schematic.get_bounding_box();
        let bounds = (
            BlockPos::new(0, 0, 0),
            BlockPos::new(bounding_box.max.0, bounding_box.max.1, bounding_box.max.2)
        );
        let options = CompilerOptions {
            optimize: true,
            io_only: true,
            wire_dot_out: true,
            ..Default::default()
        };
        let ticks = self.to_be_ticked.drain(..).collect();
        let monitor = Default::default();

        // Create a temporary Compiler
        let mut temp_compiler = std::mem::take(&mut self.compiler);

        // Use the temporary Compiler
        temp_compiler.compile(self, bounds, options, ticks, monitor);

        // Put the Compiler back
        self.compiler = temp_compiler;
    }


    fn initialize_chunks(&mut self) -> Result<(), String> {
        let bounding_box = self.schematic.get_bounding_box();
        let (min_x, min_y, min_z) = (bounding_box.min.0, bounding_box.min.1, bounding_box.min.2);
        let (max_x, max_y, max_z) = (bounding_box.max.0, bounding_box.max.1, bounding_box.max.2);

        for chunk_x in (min_x >> 4)..=((max_x >> 4) + 1) {
            for chunk_z in (min_z >> 4)..=((max_z >> 4) + 1) {
                let chunk = Chunk::empty(chunk_x, chunk_z, ((max_y - min_y) / 16 + 1) as usize);
                self.chunks.insert((chunk_x, chunk_z), chunk);
            }
        }

        // Example validation: Ensure chunks are initialized correctly
        if self.chunks.is_empty() {
            return Err("No chunks initialized".to_string());
        }

        Ok(())
    }

    fn populate_chunks(&mut self) {
        // Collect all block data first
        let block_data: Vec<_> = self.schematic.iter_blocks()
            .map(|(pos, block_state)| {
                let name = block_state.name.strip_prefix("minecraft:").unwrap_or(&block_state.name).to_string();
                let properties = block_state.properties.clone();
                let block_entity = if Block::from_name(&name).map_or(false, |b| b.has_block_entity()) {
                    self.schematic.get_block_entity(pos.clone()).cloned()
                } else {
                    None
                };
                (BlockPos::new(pos.x, pos.y, pos.z), name, properties, block_entity)
            })
            .collect();

        // Now populate the world with the collected data
        for (pos, name, properties, maybe_block_entity) in block_data {
            if let Some(mut block) = Block::from_name(&name) {
                let properties_ref: HashMap<&str, &str> = properties
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();

                block.set_properties(properties_ref);
                self.set_block_raw(pos, block.get_id());

                if let Some(util_block_entity) = maybe_block_entity {
                    if let Some(block_entity) = self.convert_block_entity(util_block_entity) {
                        self.set_block_entity(pos, block_entity);
                    }
                }
            } else {
                eprintln!("Warning: Unknown block '{}' at position {:?}", name, pos);
            }
        }
    }

    fn convert_block_entity(&self, block_entity: UtilBlockEntity) -> Option<BlockEntity> {
        let nbt = block_entity.to_hashmap();
        BlockEntity::from_nbt(&block_entity.id, &nbt)
    }

    fn get_chunk_key(&self, pos: BlockPos) -> (i32, i32) {
        (pos.x >> 4, pos.z >> 4)
    }

    pub fn update_redstone(&mut self) {
        let dimensions = self.schematic.get_dimensions();
        for x in 0..dimensions.0 {
            for y in 0..dimensions.1 {
                for z in 0..dimensions.2 {
                    let pos = BlockPos::new(x, y, z);
                    let block = self.get_block(pos);
                    mchprs_redstone::update(block, self, pos);
                }
            }
        }
    }

    pub fn get_redstone_power(&self, pos: BlockPos) -> u8 {
        self.get_block(pos)
            .properties()
            .get("power")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    pub fn set_redstone_power(&mut self, pos: BlockPos, power: u8) {
        let mut block = self.get_block(pos);
        if block.get_name() == "redstone_wire" {
            let mut properties: HashMap<&str, String> = block.properties().iter()
                .map(|(k, v)| (*k, v.to_string()))
                .collect();
            properties.insert("power", power.to_string());
            block.set_properties(properties.iter().map(|(k, v)| (*k, v.as_str())).collect());
            self.set_block_raw(pos, block.get_id());
        }
    }

    pub fn on_use_block(&mut self, pos: BlockPos) {
        let block = self.get_block(pos);
        if block.get_name() == "lever" {
            let current_state = self.get_lever_power(pos);
            self.set_lever_power(pos, !current_state);
            self.compiler.on_use_block(pos);
            return;
        }
        //we clicked on a block that is not a lever so we need to throw an error
        eprintln!("Error: Tried to use block at {:?} which is not a lever", pos);
    }

    pub fn tick(&mut self, number_of_ticks: u32) {
        // self.compiler.tick();
        for _ in 0..number_of_ticks {
            self.compiler.tick();
        }
    }

    pub fn flush(&mut self) {
        let mut temp_compiler = std::mem::take(&mut self.compiler);
        temp_compiler.flush(self);
        self.compiler = temp_compiler;
        self.update_redstone();
    }


    pub fn is_lit(&self, pos: BlockPos) -> bool {
        self.get_block(pos)
            .properties()
            .get("lit")
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    pub fn set_lever_power(&mut self, pos: BlockPos, powered: bool) {
        let mut block = self.get_block(pos);
        let mut properties: HashMap<&str, String> = block.properties().iter()
            .map(|(k, v)| (*k, v.to_string()))
            .collect();
        properties.insert("powered", powered.to_string());
        block.set_properties(properties.iter().map(|(k, v)| (*k, v.as_str())).collect());
        self.set_block_raw(pos, block.get_id());
    }

    pub fn get_lever_power(&self, pos: BlockPos) -> bool {
        self.get_block(pos)
            .properties()
            .get("powered")
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    pub fn get_compiled_world(&mut self) -> Compiler {
        let mut compiler = Compiler::default();
        let bounding_box = self.schematic.get_bounding_box();
        let bounds = (BlockPos::new(0, 0, 0), BlockPos::new(bounding_box.max.0, bounding_box.max.1, bounding_box.max.2));
        let options = CompilerOptions {
            optimize: true,
            io_only: true,
            wire_dot_out: true,
            ..Default::default()
        };
        let ticks = self.to_be_ticked.drain(..).collect();
        let monitor = Default::default();
        compiler.compile(self, bounds, options, ticks, monitor);
        compiler
    }
}

pub fn generate_truth_table(schematic: &UniversalSchematic) -> Vec<HashMap<String, bool>> {
    let mut world = MchprsWorld::new(schematic.clone());

    // Find all levers and lamps
    let (inputs, outputs) = find_inputs_and_outputs(&world);

    println!("Inputs: {:?}", inputs);
    println!("Outputs: {:?}", outputs);

    let mut truth_table = Vec::new();

    // Generate all possible input combinations
    let input_combinations = generate_input_combinations(inputs.len());

    for combination in input_combinations {
        // Set the inputs according to the current combination
        for (i, &input_pos) in inputs.iter().enumerate() {
            if combination[i] {
                world.on_use_block(input_pos);
            }
        }

        // Run the simulation
        world.tick(20);  // Adjust the number of ticks as needed
        world.flush();

        // Read the outputs
        let mut result = HashMap::new();
        for (i, &input_pos) in inputs.iter().enumerate() {
            result.insert(format!("Input {}", i), world.get_lever_power(input_pos));
        }
        for (i, &output_pos) in outputs.iter().enumerate() {
            result.insert(format!("Output {}", i), world.is_lit(output_pos));
        }

        truth_table.push(result);

        // Reset the world for the next iteration
        world = MchprsWorld::new(schematic.clone());
    }

    truth_table
}

fn find_inputs_and_outputs(world: &MchprsWorld) -> (Vec<BlockPos>, Vec<BlockPos>) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();

    let dimensions = world.schematic.get_dimensions();
    for x in 0..dimensions.0 {
        for y in 0..dimensions.1 {
            for z in 0..dimensions.2 {
                let pos = BlockPos::new(x, y, z);
                let block = world.get_block(pos);
                let block_name = block.get_name();
                match block_name {
                    "lever" => inputs.push(pos),
                    "redstone_lamp" => outputs.push(pos),
                    _ => {}
                }
            }
        }
    }

    (inputs, outputs)
}

fn generate_input_combinations(num_inputs: usize) -> Vec<Vec<bool>> {
    (0..2usize.pow(num_inputs as u32))
        .map(|i| (0..num_inputs).map(|j| (i & (1 << j)) != 0).collect())
        .collect()
}
impl World for MchprsWorld {
    fn get_block_raw(&self, pos: BlockPos) -> u32 {
        let chunk_key = self.get_chunk_key(pos);
        if let Some(chunk) = self.chunks.get(&chunk_key) {
            chunk.get_block((pos.x & 15) as u32, pos.y as u32, (pos.z & 15) as u32)
        } else {
            0 // Air
        }
    }

    fn set_block_raw(&mut self, pos: BlockPos, block: u32) -> bool {
        let chunk_key = self.get_chunk_key(pos);
        if let Some(chunk) = self.chunks.get_mut(&chunk_key) {
            chunk.set_block((pos.x & 15) as u32, pos.y as u32, (pos.z & 15) as u32, block)
        } else {
            false
        }
    }

    fn delete_block_entity(&mut self, pos: BlockPos) {
        let chunk_key = self.get_chunk_key(pos);
        if let Some(chunk) = self.chunks.get_mut(&chunk_key) {
            chunk.delete_block_entity(BlockPos::new(pos.x & 15, pos.y, pos.z & 15));
        }
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<&BlockEntity> {
        let chunk_key = self.get_chunk_key(pos);
        self.chunks.get(&chunk_key)?.get_block_entity(BlockPos::new(pos.x & 15, pos.y, pos.z & 15))
    }

    fn set_block_entity(&mut self, pos: BlockPos, block_entity: BlockEntity) {
        let chunk_key = self.get_chunk_key(pos);
        if let Some(chunk) = self.chunks.get_mut(&chunk_key) {
            chunk.set_block_entity(BlockPos::new(pos.x & 15, pos.y, pos.z & 15), block_entity);
        }
    }

    fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        self.chunks.get(&(x, z))
    }

    fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        self.chunks.get_mut(&(x, z))
    }

    fn schedule_tick(&mut self, pos: BlockPos, delay: u32, priority: TickPriority) {
        self.to_be_ticked.push(TickEntry {
            pos,
            ticks_left: delay,
            tick_priority: priority,
        });
    }

    fn pending_tick_at(&mut self, _pos: BlockPos) -> bool {
        self.to_be_ticked.iter().any(|entry| entry.pos == _pos)
    }
}


#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use super::*;
    use crate::{schematic, BlockPosition, BlockState};
    use crate::universal_schematic::SimpleBlockMapping;

    fn get_sample_schematic() -> UniversalSchematic {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        for x in 1..15 {
            schematic.set_block(x, 1, 0, BlockState::new("minecraft:redstone_wire".to_string())
                .with_properties([
                    ("power", "0"),
                    ("east", if x < 15 { "side" } else { "none" }),
                    ("west", "side"),
                    ("north", "none"),
                    ("south", "none")
                ].iter().cloned().map(|(a, b)| (a.to_string(), b.to_string())).collect()
                ));
        }
        for x in 0..16 {
            schematic.set_block(x, 0, 0, BlockState::new("minecraft:gray_concrete".to_string()));
        }
        schematic.set_block(0, 1, 0, BlockState::new("minecraft:lever".to_string())
            .with_properties([
                ("facing", "east"),
                ("powered", "true"),
                ("face", "floor")
            ].iter().cloned().map(|(a, b)| (a.to_string(), b.to_string())).collect()
            ));
        schematic.set_block(15, 1, 0, BlockState::new("minecraft:redstone_lamp".to_string())
            .with_properties([
                ("lit", "false")
            ].iter().cloned().map(|(a, b)| (a.to_string(), b.to_string())).collect()
            ));

        let schematic_file = schematic::to_schematic(&schematic).expect("Failed to convert to schem");

        let output_path = "tests/output/compiled_simple_redstone_line.schem";
        std::fs::write(output_path, &schematic_file).expect("Failed to write schematic file");
        schematic
    }

    fn get_sample_and_gate_schematic() -> UniversalSchematic {
        let input_path_str = "tests/samples/and.schem";
        let schem_path = Path::new(&input_path_str);
        let schem_data = fs::read(schem_path).expect(format!("Failed to read {}", input_path_str).as_str());
        let mut schematic = schematic::from_schematic(&schem_data).expect("Failed to parse schem");
        let redstone_lamp_block = BlockState::new("minecraft:redstone_lamp".to_string())
            .with_properties([
                ("lit", "false")
            ].iter().cloned().map(|(a, b)| (a.to_string(), b.to_string())).collect());
        schematic.set_block(1, 0, 3, redstone_lamp_block.clone());

        // save the schematic
        let schematic_file = schematic::to_schematic(&schematic).expect("Failed to convert to schem");
        let output_path = "tests/output/compiled_and_gate.schem";
        std::fs::write(output_path, &schematic_file).expect("Failed to write schematic file");
        schematic
    }

    #[test]
    fn test_simple_redstone_line() {
        let schematic = get_sample_schematic();
        let mut world = MchprsWorld::new(schematic);

        for x in 1..15 {
            let power = world.get_redstone_power(BlockPos::new(x, 1, 0));
            assert_eq!(power, 16 - x as u8);
        }
        println!("{:?}", world.get_block(BlockPos::new(0, 1, 0)).properties());

        assert_eq!(world.is_lit(BlockPos::new(15, 1, 0)), true);

        world.on_use_block(BlockPos::new(0, 1, 0));
        println!("{:?}", world.get_block(BlockPos::new(0, 1, 0)).properties());

        world.tick(2);
        world.flush();
        assert_eq!(world.is_lit(BlockPos::new(15, 1, 0)), false);

        world.on_use_block(BlockPos::new(0, 1, 0));
        world.tick(2);
        world.flush();

        assert_eq!(world.is_lit(BlockPos::new(15, 1, 0)), true);
        world.on_use_block(BlockPos::new(0, 1, 0));
        world.tick(1);
        world.flush();

        assert_eq!(world.is_lit(BlockPos::new(15, 1, 0)), true);
        world.tick(1);
        world.flush();
        assert_eq!(world.is_lit(BlockPos::new(15, 1, 0)), false);
    }

    #[test]
    fn test_simple_and_gate() {
        let schematic = get_sample_and_gate_schematic();
        let mut world = MchprsWorld::new(schematic);

        let lever_a_pos = BlockPos::new(0, 0, 0);
        let lever_b_pos = BlockPos::new(2, 0, 0);
        let output_lamp_pos = BlockPos::new(1, 0, 3);

        for a in 0..2 {
            for b in 0..2 {
                let lever_a_state = world.get_lever_power(lever_a_pos);
                let lever_b_state = world.get_lever_power(lever_b_pos);
                if lever_a_state != (a == 1) {
                    world.on_use_block(lever_a_pos);
                }
                if lever_b_state != (b == 1) {
                    world.on_use_block(lever_b_pos);
                }

                world.tick(2);
                world.flush();
                println!("A: {}, B: {}, Output: {}", a, b, world.is_lit(output_lamp_pos));
                assert_eq!(world.is_lit(output_lamp_pos), a == 1 && b == 1);
            }
        }
    }

    fn get_comparator_xor_gate() -> UniversalSchematic {
        let block_mappings: &[(&char, SimpleBlockMapping)] = &[
            (&'C', ("gray_concrete", vec![])),
            (&'X', ("comparator", vec![
                ("mode", "subtract"),
                ("powered", "false"),
                ("facing", "south")
            ])),
            (&'R', ("redstone_wire", vec![
                ("power", "0"),
                ("north", "side"),
                ("south", "side"),
                ("east", "side"),
                ("west", "side")
            ])),
            (&'.', ("air", vec![])),
            (&'L', ("redstone_lamp", vec![("lit", "false")])),
            (&'l', ("lever", vec![
                ("powered", "false"),
                ("face", "wall"),
                ("facing", "south")
            ])),
        ];

        let layers = r#"
        CCCC
        CCCC
        CCCC
        CCCC
        ....

        .L..
        .RC.
        RXXR
        CRRC
        l..l
    "#;

        let schematic = UniversalSchematic::from_layers("XOR gate".to_string(), block_mappings, layers);

        // Save the schematic
        let schematic_file = crate::schematic::to_schematic(&schematic).expect("Failed to convert to schem");
        std::fs::write("tests/output/compiled_xor_gate.schem", &schematic_file)
            .expect("Failed to write schematic file");

        schematic
    }

    #[test]
    fn test_torch_based_xor() {
        let block_mappings: &[(&char, SimpleBlockMapping)] = &[
            (&'C', ("gray_concrete", vec![])),
            (&'I', ("redstone_wall_torch", vec![
                ("lit", "false"),
                ("facing", "north")
            ])),
            (&'i', ("redstone_torch", vec![
                ("lit", "false"),
            ])),
            (&'R', ("redstone_wire", vec![
                ("power", "0"),
                ("north", "side"),
                ("south", "side"),
                ("east", "side"),
                ("west", "side")
            ])),
            (&'.', ("air", vec![])),
            (&'L', ("redstone_lamp", vec![("lit", "false")])),
            (&'l', ("lever", vec![
                ("powered", "false"),
                ("face", "wall"),
                ("facing", "south")
            ])),
        ];
        let layers = r#"
        ILI
        C.C
        C.C
        l.l

        ...
        RIR
        iCi
        ...

        ...
        ...
        CRC
        ...
    "#;

        let schematic = UniversalSchematic::from_layers("XOR gate".to_string(), block_mappings, layers);
        for row in &generate_truth_table(&schematic) {
            assert_eq!(*row.get("Output 0").unwrap(), *row.get("Input 0").unwrap() ^ *row.get("Input 1").unwrap());
        }
    }


    #[test]
    fn test_comparator_xor_gate() {
        let schematic = get_comparator_xor_gate();
        let mut world = MchprsWorld::new(schematic);

        let lever_a_pos = BlockPos::new(0, 1, 4);
        let lever_b_pos = BlockPos::new(3, 1, 4);
        let output_lamp_pos = BlockPos::new(1, 1, 0);

        for a in 0..2 {
            for b in 0..2 {
                let lever_a_state = world.get_lever_power(lever_a_pos);
                let lever_b_state = world.get_lever_power(lever_b_pos);
                if lever_a_state != (a == 1) {
                    world.on_use_block(lever_a_pos);
                }
                if lever_b_state != (b == 1) {
                    world.on_use_block(lever_b_pos);
                }
                world.tick(4);
                world.flush();
                println!("A: {}, B: {}, Output: {}", a, b, world.is_lit(output_lamp_pos));
                assert_eq!(world.is_lit(output_lamp_pos), a != b);
            }
        }
    }
    #[test]
    fn test_auto_truth_table_xor_gate() {
        let schematic = get_comparator_xor_gate();
        let truth_table = generate_truth_table(&schematic);

        println!("XOR Gate Truth Table:");
        for row in &truth_table {
            println!("{:?}", row);
        }

        assert_eq!(truth_table.len(), 4);  // 2^2 combinations for 2 inputs

        // Verify XOR behavior
        for row in truth_table {
            let input_a = row.get("Input 0").unwrap();
            let input_b = row.get("Input 1").unwrap();
            let output = row.get("Output 0").unwrap();

            assert_eq!(*output, *input_a ^ *input_b);
        }
    }

    #[test]
    fn test_auto_truth_table_and_gate() {
        let schematic = get_sample_and_gate_schematic();
        let truth_table = generate_truth_table(&schematic);

        println!("AND Gate Truth Table:");
        for row in &truth_table {
            println!("{:?}", row);
        }

        assert_eq!(truth_table.len(), 4);  // 2^2 combinations for 2 inputs

        // Verify AND behavior
        for row in truth_table {
            let input_a = row.get("Input 0").unwrap();
            let input_b = row.get("Input 1").unwrap();
            let output = row.get("Output 0").unwrap();

            assert_eq!(*output, *input_a & *input_b);
        }
    }
}