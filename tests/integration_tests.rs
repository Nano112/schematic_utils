use std::fs;
use std::path::Path;
use minecraft_schematic_utils::{litematic, schematic};

#[test]
fn test_litematic_to_schem_conversion() {
    // Path to the sample .litematic file
    let litematic_path = Path::new("tests/samples/sample.litematic");

    // Ensure the sample file exists
    assert!(litematic_path.exists(), "Sample .litematic file not found");

    // Read the .litematic file
    let litematic_data = fs::read(litematic_path).expect("Failed to read sample.litematic");

    // Parse the .litematic data into a UniversalSchematic
    let schematic = litematic::from_litematic(&litematic_data).expect("Failed to parse litematic");

    // Convert the UniversalSchematic to .schem format
    let schem_data = schematic::to_schematic(&schematic).expect("Failed to convert to schem");

    // Save the .schem file
    let schem_path = Path::new("tests/output/sample_converted.schem");
    fs::write(schem_path, &schem_data).expect("Failed to write schem file");

    // Optionally, read back the .schem file and compare
    let read_back_data = fs::read(schem_path).expect("Failed to read back schem file");
    let read_back_schematic = schematic::from_schematic(&read_back_data).expect("Failed to parse schem");

    // Compare original and converted schematics
    assert_eq!(schematic.metadata.name, read_back_schematic.metadata.name);
    assert_eq!(schematic.regions.len(), read_back_schematic.regions.len());

    // Compare the first region (assuming there's at least one)
    if let (Some(original_region), Some(converted_region)) = (schematic.regions.values().next(), read_back_schematic.regions.values().next()) {
        assert_eq!(original_region.size, converted_region.size);
        assert_eq!(original_region.count_blocks(), converted_region.count_blocks());

        // Check a few random blocks
        let (width, height, length) = original_region.size;
        for _ in 0..10 {
            let x = rand::random::<i32>() % width;
            let y = rand::random::<i32>() % height;
            let z = rand::random::<i32>() % length;
            assert_eq!(
                original_region.get_block(x, y, z),
                converted_region.get_block(x, y, z),
                "Mismatch at coordinates ({}, {}, {})", x, y, z
            );
        }
    }

    // Clean up the generated file
    fs::remove_file(schem_path).expect("Failed to remove converted schem file");

    println!("Successfully converted sample.litematic to .schem format and verified the contents.");
}