use std::fs;
use std::path::Path;
use minecraft_schematic_utils::{BlockState, litematic, print_json_schematic, schematic, UniversalSchematic};

#[test]
fn test_litematic_to_schem_conversion() {
    let name = "sample";
    let start_time = std::time::Instant::now();

    // Path to the sample .litematic file
    let input_path_str = format!("tests/samples/{}.litematic", name);
    let litematic_path = Path::new(&input_path_str);

    // Ensure the sample file exists
    assert!(litematic_path.exists(), "Sample .litematic file not found");

    // Read the .litematic file
    let litematic_data = fs::read(litematic_path).expect(format!("Failed to read {}", input_path_str).as_str());
    println!("Read litematic file in {:.2?}", start_time.elapsed());
    // Parse the .litematic data into a UniversalSchematic
    let mut schematic = litematic::from_litematic(&litematic_data).expect("Failed to parse litematic");
    println!("Parsed litematic file in {:.2?}", start_time.elapsed());
    // Get dimensions of the schematic
    let dimensions = schematic.get_dimensions();
    println!("Dimensions: {:?}", dimensions);
    let (width, height, length) = dimensions;

    // Add a layer of gray concrete at the bottom
    for x in 0..width {
        for z in 0..length {
            schematic.set_block(x, -1, z, BlockState::new("minecraft:gray_concrete".to_string()));
        }
    }
    println!("Added gray concrete layer in {:.2?}", start_time.elapsed());

    // Convert the UniversalSchematic to .schem format
    let schem_data = schematic::to_schematic(&schematic).expect("Failed to convert to schem");

    // Save the .schem file
    let output_schem_path = format!("tests/output/{}.schem", name);
    let schem_path = Path::new(&output_schem_path);
    fs::write(schem_path, &schem_data).expect("Failed to write schem file");

    // Convert the UniversalSchematic back to .litematic format
    let litematic_data = litematic::to_litematic(&schematic).expect("Failed to convert to litematic");

    println!("Time taken_1: {:.2?}", start_time.elapsed());
    // Save the .litematic file
    let output_litematic_path = format!("tests/output/{}.litematic", name);
    let litematic_path = Path::new(&output_litematic_path);
    fs::write(litematic_path, &litematic_data).expect("Failed to write litematic file");
    println!("Time taken_2: {:.2?}", start_time.elapsed());
    // Read back the .schem file and compare
    let read_back_data = fs::read(schem_path).expect("Failed to read back schem file");
    let read_back_schematic = schematic::from_schematic(&read_back_data).expect("Failed to parse schem");
    println!("Time taken_3: {:.2?}", start_time.elapsed());
    // Compare original and converted schematics
    assert_eq!(schematic.metadata.name, read_back_schematic.metadata.name);

    // Verify the dimensions
    let read_back_dimensions = read_back_schematic.get_dimensions();
    assert_eq!(dimensions, read_back_dimensions, "Dimensions mismatch after conversion");

    // Verify the added gray concrete layer
    for x in 0..width {
        for z in 0..length {
            let block = read_back_schematic.get_block(x, -1, z);
            assert_eq!(block, Some(&BlockState::new("minecraft:gray_concrete".to_string())),
                       "Gray concrete not found at expected position ({}, -1, {})", x, z);
        }
    }

    println!("Successfully converted sample.litematic to .schem format and verified the contents.");
}

#[test]
fn test_schem_to_litematic_conversion() {
    let name = "sample";

    // Path to the sample .schem file
    let input_path_str = format!("tests/samples/{}.schem", name);
    let schem_path = Path::new(&input_path_str);

    // Ensure the sample file exists
    assert!(schem_path.exists(), "Sample .schem file not found");

    // Read the .schem file
    let schem_data = fs::read(schem_path).expect(format!("Failed to read {}", input_path_str).as_str());

    // Parse the .schem data into a UniversalSchematic
    let schematic = schematic::from_schematic(&schem_data).expect("Failed to parse schem");

    // Convert the UniversalSchematic to .litematic format
    let litematic_data = litematic::to_litematic(&schematic).expect("Failed to convert to litematic");

    // Save the .litematic file
    let output_litematic_path = format!("tests/output/{}.litematic", name);
    let litematic_path = Path::new(&output_litematic_path);
    fs::write(litematic_path, &litematic_data).expect("Failed to write litematic file");

    // Read back the .litematic file and compare
    let read_back_data = fs::read(litematic_path).expect("Failed to read back litematic file");
    let read_back_schematic = litematic::from_litematic(&read_back_data).expect("Failed to parse litematic");

    // Compare original and converted schematics
    assert_eq!(schematic.metadata.name, read_back_schematic.metadata.name);
    assert_eq!(schematic.regions.len(), read_back_schematic.regions.len());

    // Verify the dimensions
    let original_dimensions = schematic.get_dimensions();
    let read_back_dimensions = read_back_schematic.get_dimensions();
    assert_eq!(original_dimensions, read_back_dimensions, "Dimensions mismatch after conversion");

    // Verify block consistency
    let (width, height, length) = original_dimensions;
    for x in 0..width {
        for y in 0..height {
            for z in 0..length {
                let original_block = schematic.get_block(x, y, z);
                let converted_block = read_back_schematic.get_block(x, y, z);
                assert_eq!(original_block, converted_block,
                           "Block mismatch at ({}, {}, {})", x, y, z);
            }
        }
    }

    println!("Successfully converted sample.schem to .litematic format and verified the contents.");
}

fn run_schematic_roundtrip_test(width: i32, height: i32, length: i32) {
    let test_name = format!("{}x{}x{}", width, height, length);
    println!("Running test for dimensions: {}", test_name);

    // Create a schematic with the given dimensions
    let mut original_schematic = UniversalSchematic::new(format!("Test Schematic {}", test_name));
    let stone = BlockState::new("minecraft:stone".to_string());
    let dirt = BlockState::new("minecraft:dirt".to_string());

    // Set blocks in the schematic
    for x in 0..width {
        for y in 0..height {
            for z in 0..length {
                if (x + y + z) % 2 == 0 {
                    original_schematic.set_block(x, y, z, stone.clone());
                } else {
                    original_schematic.set_block(x, y, z, dirt.clone());
                }
            }
        }
    }

    // Convert the schematic to .schem format
    let schem_data = schematic::to_schematic(&original_schematic).expect("Failed to convert to schem");

    // Create a String for the file path
    let file_path = format!("tests/output/test_roundtrip_{}.schem", test_name);

    // Save the .schem file
    let output_path = Path::new(&file_path);
    fs::write(output_path, &schem_data).expect("Failed to write schem file");

    // Read the .schem file back
    let read_data = fs::read(output_path).expect("Failed to read schem file");

    // Parse the loaded .schem data
    let loaded_schematic = schematic::from_schematic(&read_data).expect("Failed to parse schem");

    // Compare dimensions
    let original_dimensions = original_schematic.get_dimensions();
    let loaded_dimensions = loaded_schematic.get_dimensions();
    assert_eq!(original_dimensions, loaded_dimensions, "Dimensions mismatch after roundtrip for {}", test_name);

    // Compare blocks
    for x in 0..width {
        for y in 0..height {
            for z in 0..length {
                let original_block = original_schematic.get_block(x, y, z);
                let loaded_block = loaded_schematic.get_block(x, y, z);
                assert_eq!(original_block, loaded_block, "Block mismatch at ({}, {}, {}) for {}", x, y, z, test_name);
            }
        }
    }

    // Clean up
    fs::remove_file(output_path).expect("Failed to remove test file");

    println!("Successfully completed schematic roundtrip test for {}", test_name);
}

#[test]
fn test_schematic_roundtrip_multiple_dimensions() {
    let test_cases = vec![
        (1, 1, 1),    // Minimum size
        (3, 3, 3),    // Small cube
        (16, 16, 16), // Standard chunk size
        (32, 32, 32), // Large cube
        (64, 64, 64), // Very large cube
        (10, 5, 20),  // Non-cubic dimensions
        (7, 13, 17),  // Prime number dimensions
        (256, 1, 1),  // Long in one dimension
        (1, 256, 1),  // Tall and thin
        (1, 1, 256),  // Deep and narrow
    ];

    for (width, height, length) in test_cases {
        run_schematic_roundtrip_test(width, height, length);
    }
}