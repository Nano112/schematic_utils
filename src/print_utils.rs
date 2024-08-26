use crate::{UniversalSchematic, BlockState};
use crate::metadata::Metadata;
use crate::region::Region;

impl std::fmt::Debug for UniversalSchematic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UniversalSchematic")
            .field("metadata", &self.metadata)
            .field("regions", &self.regions.keys().collect::<Vec<_>>())
            .finish()
    }
}

pub fn format_schematic(schematic: &UniversalSchematic) -> String {
    let mut output = String::new();
    output.push_str("Schematic:\n");
    output.push_str(&format_metadata(&schematic.metadata));
    output.push_str("Regions:\n");
    for (name, region) in &schematic.regions {
        output.push_str(&format_region(name, region));
    }
    output
}

pub fn get_schematic_json(schematic: &UniversalSchematic) -> String {
    schematic.get_json_string().unwrap_or_else(|e| format!("Failed to serialize: {}", e))
}

pub fn format_palette(palette: &Vec<BlockState>) -> String {
    let mut output = String::from("Palette:\n");
    for (i, block) in palette.iter().enumerate() {
        output.push_str(&format!("  {}: {}\n", i, block.name));
    }
    output
}

pub fn format_region(name: &str, region: &Region) -> String {
    let mut output = String::new();
    output.push_str(&format!("  Region: {}\n", name));
    output.push_str(&format!("    Position: {:?}\n", region.position));
    output.push_str(&format!("    Size: {:?}\n", region.size));
    output.push_str("    Blocks:\n");
    for i in 0..region.blocks.len() {
        let block_palette_index = region.blocks[i];
        let block_position = region.index_to_coords(i);
        let block_state = region.palette.get(block_palette_index as usize).unwrap();
        output.push_str(&format!("      {} @ {:?}: {:?}\n", block_palette_index, block_position, block_state));
    }
    output
}

pub fn format_metadata(metadata: &Metadata) -> String {
    let mut output = String::from("Metadata:\n");
    if let Some(author) = &metadata.author {
        output.push_str(&format!("  Author: {}\n", author));
    }
    if let Some(name) = &metadata.name {
        output.push_str(&format!("  Name: {}\n", name));
    }
    if let Some(description) = &metadata.description {
        output.push_str(&format!("  Description: {}\n", description));
    }
    if let Some(created) = metadata.created {
        output.push_str(&format!("  Created: {}\n", created));
    }
    if let Some(modified) = metadata.modified {
        output.push_str(&format!("  Modified: {}\n", modified));
    }
    if let Some(mc_version) = metadata.mc_version {
        output.push_str(&format!("  Minecraft Version: {}\n", mc_version));
    }
    if let Some(we_version) = metadata.we_version {
        output.push_str(&format!("  WorldEdit Version: {}\n", we_version));
    }
    output
}

pub fn format_json_schematic(schematic: &UniversalSchematic) -> String {
    match schematic.get_json_string() {
        Ok(json) => json,
        Err(e) => format!("Failed to serialize: {}", e),
    }
}

pub fn format_block_state(block: &BlockState) -> String {
    let mut output = format!("Block: {}\n", block.name);
    if !block.properties.is_empty() {
        output.push_str("Properties:\n");
        for (key, value) in &block.properties {
            output.push_str(&format!("  {}: {}\n", key, value));
        }
    }
    output
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

        let json = get_schematic_json(&schematic);
        println!("{}", json);

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
        println!("{}", format_schematic(&schematic));

        // This will print details of a specific block state
        println!("{}", format_block_state(&stone));

        // Test with a custom region
        schematic.set_block_in_region("Custom", 5, 5, 5, stone.clone());
        println!("{}", format_schematic(&schematic));
    }
}