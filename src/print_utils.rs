use crate::{UniversalSchematic, Region, BlockState};
use crate::metadata::Metadata;

impl std::fmt::Debug for UniversalSchematic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UniversalSchematic")
            .field("metadata", &self.metadata)
            .field("regions", &self.regions.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[allow(dead_code)]
pub fn print_schematic(schematic: &UniversalSchematic) {
    println!("Schematic:");
    print_metadata(&schematic.metadata);
    println!("Regions:");
    for (name, region) in &schematic.regions {
        print_region(name, region, schematic);
    }
}

#[allow(dead_code)]
pub fn print_palette(palette: &Vec<BlockState>) {
    println!("Palette:");
    for (i, block) in palette.iter().enumerate() {
        println!("  {}: {}", i, block.name);
    }
}

#[allow(dead_code)]
pub fn print_region(name: &str, region: &Region, schematic: &UniversalSchematic) {
    println!("  Region: {}", name);

    println!("    Position: {:?}", region.position);
    println!("    Size: {:?}", region.size);
    println!("    Blocks:");
    for i in 0..region.blocks.len() {
        let block_palette_index = region.blocks[i];
        let block_position = region.index_to_coords(i);
        let block_state = region.palette.get(block_palette_index as usize).unwrap();
        println!("      {} @ {:?}: {:?}", block_palette_index, block_position, block_state);
    }
}
#[allow(dead_code)]
pub fn print_metadata(metadata: &Metadata) {
    println!("Metadata:");
    if let Some(author) = &metadata.author {
        println!("  Author: {}", author);
    }
    if let Some(name) = &metadata.name {
        println!("  Name: {}", name);
    }
    if let Some(description) = &metadata.description {
        println!("  Description: {}", description);
    }
    if let Some(created) = metadata.created {
        println!("  Created: {}", created);
    }
    if let Some(modified) = metadata.modified {
        println!("  Modified: {}", modified);
    }
    if let Some(mc_version) = metadata.mc_version {
        println!("  Minecraft Version: {}", mc_version);
    }
    if let Some(we_version) = metadata.we_version {
        println!("  WorldEdit Version: {}", we_version);
    }
}

#[allow(dead_code)]
pub fn print_json_schematic(schematic: &UniversalSchematic) {
    match schematic.get_json_string() {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Failed to serialize: {}", e),
    }
}

#[allow(dead_code)]
pub fn print_block_state(block: &BlockState) {
    println!("Block: {}", block.name);
    if !block.properties.is_empty() {
        println!("Properties:");
        for (key, value) in &block.properties {
            println!("  {}: {}", key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schematic_debug_json_print() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        schematic.set_block(0, 0, 0, stone.clone());
        schematic.set_block(1, 1, 1, dirt.clone());

        match schematic.get_json_string() {
            Ok(json) => println!("{}", json),
            Err(e) => eprintln!("Failed to serialize: {}", e),
        }

        println!("{:?}", schematic);
    }
    #[test]
    fn test_schematic_debug_print() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        // Set blocks in the default region
        schematic.set_block(0, 0, 0, stone.clone());
        schematic.set_block(1, 1, 1, dirt.clone());

        // This will use the Debug implementation
        println!("{:?}", schematic);

        // This will print a detailed view of the schematic
        print_schematic(&schematic);

        // This will print details of a specific block state
        print_block_state(&stone);

        // Test with a custom region
        schematic.set_block_in_region("Custom", 5, 5, 5, stone.clone());
        print_schematic(&schematic);
    }
}