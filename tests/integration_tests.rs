use std::fs;
use std::path::Path;
use minecraft_schematic_utils::{BlockState, litematic, print_json_schematic, schematic};

#[test]
fn test_litematic_to_schem_conversion() {

    let name = "all_items";

    // Path to the sample .litematic file
    let input_path_str = format!("tests/samples/{}.litematic", name);
    let litematic_path = Path::new(&input_path_str);

    // Ensure the sample file exists
    assert!(litematic_path.exists(), "Sample .litematic file not found");

    // Read the .litematic file
    let litematic_data = fs::read(litematic_path).expect(format!("Failed to read {}", input_path_str).as_str());

    // Parse the .litematic data into a UniversalSchematic
    let mut schematic = litematic::from_litematic(&litematic_data).expect("Failed to parse litematic");

    let region_blocks = schematic.get_region_from_index(0).unwrap().blocks.clone();

    //print the length of the blocks list
    println!("{:?}", region_blocks.len());
    //print the blocks list
    println!("{:?}", region_blocks);

    println!("{:?}", schematic.count_block_types());
    //place a diamond block at the center of the schematic
    // schematic.set_block(-1,-1,-1, BlockState::new("minecraft:diamond_block".to_string()));

    let dimensions = schematic.get_dimensions();
    let width = dimensions.0;
    let height = dimensions.1;
    let length = dimensions.2;
    for x in 0..width {
        for z in 0..length {
            schematic.set_block(x, -1, z, BlockState::new("minecraft:gray_concrete".to_string()));
        }
    }
    // print the schematic in json format
    let json = print_json_schematic(&schematic);
    println!("{}", json);



    // Convert the UniversalSchematic to .schem format
    let schem_data = schematic::to_schematic(&schematic).expect("Failed to convert to schem");


    // Save the .schem file
    let output_schem_path = format!("tests/output/{}.schem", name);
    let schem_path = Path::new(&output_schem_path);
    fs::write(schem_path, &schem_data).expect("Failed to write schem file");

    // Convert the UniversalSchematic back to .litematic format
    let litematic_data = litematic::to_litematic(&schematic).expect("Failed to convert to litematic");

    // Save the .litematic file
    let output_litematic_path = format!("tests/output/{}.litematic", name);
    let litematic_path = Path::new(&output_litematic_path);
    fs::write(litematic_path, &litematic_data).expect("Failed to write litematic file");



    // Optionally, read back the .schem file and compare
    let read_back_data = fs::read(schem_path).expect("Failed to read back schem file");
    let read_back_schematic = schematic::from_schematic(&read_back_data).expect("Failed to parse schem");

    // Compare original and converted schematics
    assert_eq!(schematic.metadata.name, read_back_schematic.metadata.name);
    assert_eq!(schematic.regions.len(), read_back_schematic.regions.len());



    // Clean up the generated file
    //fs::remove_file(schem_path).expect("Failed to remove converted schem file");

    println!("Successfully converted sample.litematic to .schem format and verified the contents.");
}


#[test]
fn test_schema_to_litematic_conversion() {
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
}