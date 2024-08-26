use std::collections::HashMap;
use quartz_nbt::{NbtCompound, NbtTag};
use serde::{Deserialize, Serialize};
use crate::{ BlockState};
use crate::block_entity::BlockEntity;
use crate::bounding_box::BoundingBox;
use crate::entity::Entity;
use crate::metadata::Metadata;
use crate::region::Region;

#[derive(Serialize, Deserialize)]
pub struct UniversalSchematic {
    pub metadata: Metadata,
    pub regions: HashMap<String, Region>,
    pub default_region_name: String,
}



impl UniversalSchematic {
    pub fn new(name: String) -> Self {
        UniversalSchematic {
            metadata: Metadata {
                name: Some(name),
                ..Metadata::default()
            },
            regions: HashMap::new(),
            default_region_name: "Main".to_string(),
        }
    }


    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: BlockState) -> bool {
        let region_name = self.default_region_name.clone();
        self.set_block_in_region(&region_name, x, y, z, block)
    }

    pub fn set_block_in_region(&mut self, region_name: &str, x: i32, y: i32, z: i32, block: BlockState) -> bool {
        let region = self.regions.entry(region_name.to_string()).or_insert_with(|| {
            Region::new(region_name.to_string(), (x, y, z), (1, 1, 1))
        });

        region.set_block(x, y, z, block)
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<&BlockState> {
        for region in self.regions.values() {
            if region.get_bounding_box().contains((x, y, z)) {
                return region.get_block(x, y, z);
            }
        }
        None
    }

    pub fn get_region_names(&self) -> Vec<String> {
        self.regions.keys().cloned().collect()
    }

    pub fn get_region_from_index(&self, index: usize) -> Option<&Region> {
        self.regions.values().nth(index)
    }




    pub fn get_block_from_region(&self, region_name: &str, x: i32, y: i32, z: i32) -> Option<&BlockState> {
        self.regions.get(region_name).and_then(|region| region.get_block(x, y, z))
    }

    pub fn get_dimensions(&self) -> (i32, i32, i32) {
        let bounding_box = self.get_bounding_box();
        bounding_box.get_dimensions()
    }


    pub fn get_json_string(&self) -> Result<String, String> {
        // Attempt to serialize the name
        let metadata_json = serde_json::to_string(&self.metadata)
            .map_err(|e| format!("Failed to serialize 'metadata' in UniversalSchematic: {}", e))?;

        // Attempt to serialize the regions
        let regions_json = serde_json::to_string(&self.regions)
            .map_err(|e| format!("Failed to serialize 'regions' in UniversalSchematic: {}", e))?;


        // Combine everything into a single JSON object manually
        let combined_json = format!(
            "{{\"metadata\":{},\"regions\":{}}}",
            metadata_json, regions_json
        );

        Ok(combined_json)
    }

    pub(crate) fn total_blocks(&self) -> i32 {
        self.regions.values().map(|r| r.count_blocks() as i32).sum()
    }

    pub(crate) fn total_volume(&self) -> i32 {
        self.regions.values().map(|r| r.volume() as i32).sum()
    }



    pub fn get_region_bounding_box(&self, region_name: &str) -> Option<BoundingBox> {
        self.regions.get(region_name).map(|region| region.get_bounding_box())
    }

    pub fn get_schematic_bounding_box(&self) -> Option<BoundingBox> {
        if self.regions.is_empty() {
            return None;
        }

        let mut bounding_box = self.regions.values().next().unwrap().get_bounding_box();
        for region in self.regions.values().skip(1) {
            bounding_box = bounding_box.union(&region.get_bounding_box());
        }
        Some(bounding_box)
    }


    pub fn add_region(&mut self, region: Region) -> bool {
        if self.regions.contains_key(&region.name) {
            false
        } else {
            self.regions.insert(region.name.clone(), region);
            true
        }
    }

    pub fn remove_region(&mut self, name: &str) -> Option<Region> {
        self.regions.remove(name)
    }

    pub fn get_region(&self, name: &str) -> Option<&Region> {
        self.regions.get(name)
    }

    pub fn get_region_mut(&mut self, name: &str) -> Option<&mut Region> {
        self.regions.get_mut(name)
    }

    pub fn get_merged_region(&self) -> Region {
        let mut merged_region = self.regions.values().next().unwrap().clone();

        for region in self.regions.values().skip(1) {
            merged_region.merge(region);
        }

        merged_region
    }

    pub fn add_block_entity_in_region(&mut self, region_name: &str, block_entity: BlockEntity) -> bool {
        let region = self.regions.entry(region_name.to_string()).or_insert_with(|| {
            Region::new(region_name.to_string(), block_entity.position, (1, 1, 1))
        });

        region.add_block_entity(block_entity);
        true
    }

    pub fn remove_block_entity_in_region(&mut self, region_name: &str, position: (i32, i32, i32)) -> Option<BlockEntity> {
        self.regions.get_mut(region_name)?.remove_block_entity(position)
    }

    pub fn add_block_entity(&mut self, block_entity: BlockEntity) -> bool {
        let region_name = self.default_region_name.clone();
        self.add_block_entity_in_region(&region_name, block_entity)
    }

    pub fn remove_block_entity(&mut self, position: (i32, i32, i32)) -> Option<BlockEntity> {
        let region_name = self.default_region_name.clone();
        self.remove_block_entity_in_region(&region_name, position)
    }

    pub fn add_entity_in_region(&mut self, region_name: &str, entity: Entity) -> bool {
        let region = self.regions.entry(region_name.to_string()).or_insert_with(|| {
            let rounded_position = (entity.position.0.round() as i32, entity.position.1.round() as i32, entity.position.2.round() as i32);
            Region::new(region_name.to_string(), rounded_position, (1, 1, 1))
        });

        region.add_entity(entity);
        true
    }

    pub fn remove_entity_in_region(&mut self, region_name: &str, index: usize) -> Option<Entity> {
        self.regions.get_mut(region_name)?.remove_entity(index)
    }

    pub fn add_entity(&mut self, entity: Entity) -> bool {
        let region_name = self.default_region_name.clone();
        self.add_entity_in_region(&region_name, entity)
    }

    pub fn remove_entity(&mut self, index: usize) -> Option<Entity> {
        let region_name = self.default_region_name.clone();
        self.remove_entity_in_region(&region_name, index)
    }

    pub fn to_nbt(&self) -> NbtCompound {
        let mut root = NbtCompound::new();

        root.insert("Metadata", self.metadata.to_nbt());

        let mut regions_tag = NbtCompound::new();
        for (name, region) in &self.regions {
            regions_tag.insert(name, region.to_nbt());
        }
        root.insert("Regions", NbtTag::Compound(regions_tag));

        root.insert("DefaultRegion", NbtTag::String(self.default_region_name.clone()));

        root
    }

    pub fn from_nbt(nbt: NbtCompound) -> Result<Self, String> {
        let metadata = Metadata::from_nbt(nbt.get::<_, &NbtCompound>("Metadata")
            .map_err(|e| format!("Failed to get Metadata: {}", e))?)?;

        let regions_tag = nbt.get::<_, &NbtCompound>("Regions")
            .map_err(|e| format!("Failed to get Regions: {}", e))?;
        let mut regions = HashMap::new();
        for (region_name, region_tag) in regions_tag.inner() {
            if let NbtTag::Compound(region_compound) = region_tag {
                regions.insert(region_name.to_string(), Region::from_nbt(&region_compound.clone())?);
            }
        }

        let default_region_name = nbt.get::<_, &str>("DefaultRegion")
            .map_err(|e| format!("Failed to get DefaultRegion: {}", e))?
            .to_string();

        Ok(UniversalSchematic {
            metadata,
            regions,
            default_region_name,
        })
    }


    pub fn get_bounding_box(&self) -> BoundingBox {
        if self.regions.is_empty() {
            return BoundingBox::new((0, 0, 0), (0, 0, 0));
        }
        let mut bounding_box = BoundingBox::new((i32::MAX, i32::MAX, i32::MAX), (i32::MIN, i32::MIN, i32::MIN));

        for region in self.regions.values() {
            let region_bb = region.get_bounding_box();
            bounding_box = bounding_box.union(&region_bb);
        }

        bounding_box
    }

    pub fn to_schematic(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        crate::formats::schematic::to_schematic(self)
    }

    pub fn from_schematic(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        crate::formats::schematic::from_schematic(data)
    }

    pub fn count_block_types(&self) -> HashMap<BlockState, usize> {
        let mut block_counts = HashMap::new();
        for region in self.regions.values() {
            let region_block_counts = region.count_block_types();
            for (block, count) in region_block_counts {
                *block_counts.entry(block).or_insert(0) += count;
            }
        }
        block_counts
    }


}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use quartz_nbt::io::{read_nbt, write_nbt};
    use super::*;




    #[test]
    fn test_schematic_operations() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        // Test automatic region creation and expansion
        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        assert!(schematic.set_block(0, 0, 0, stone.clone()));
        assert_eq!(schematic.get_block(0, 0, 0), Some(&stone));

        assert!(schematic.set_block(5, 5, 5, dirt.clone()));
        assert_eq!(schematic.get_block(5, 5, 5), Some(&dirt));

        // Check that the default region was created and expanded
        let default_region = schematic.get_region("Main").unwrap();

        // Test explicit region creation and manipulation
        let obsidian = BlockState::new("minecraft:obsidian".to_string());
        assert!(schematic.set_block_in_region("Custom", 10, 10, 10, obsidian.clone()));
        assert_eq!(schematic.get_block_from_region("Custom", 10, 10, 10), Some(&obsidian));

        // Check that the custom region was created
        let custom_region = schematic.get_region("Custom").unwrap();
        assert_eq!(custom_region.position, (10, 10, 10));

        // Test manual region addition
        let region2 = Region::new("Region2".to_string(), (20, 0, 0), (5, 5, 5));
        assert!(schematic.add_region(region2));
        assert!(!schematic.add_region(Region::new("Region2".to_string(), (0, 0, 0), (1, 1, 1))));

        // Test getting non-existent blocks
        assert_eq!(schematic.get_block(100, 100, 100), None);
        assert_eq!(schematic.get_block_from_region("NonexistentRegion", 0, 0, 0), None);

        // Test removing regions
        assert!(schematic.remove_region("Region2").is_some());
        assert!(schematic.remove_region("Region2").is_none());

        // Test that removed region's blocks are no longer accessible
        assert_eq!(schematic.get_block_from_region("Region2", 20, 0, 0), None);
    }

    #[test]
    fn test_schematic_large_coordinates() {
        let mut schematic = UniversalSchematic::new("Large Schematic".to_string());

        let far_block = BlockState::new("minecraft:diamond_block".to_string());
        assert!(schematic.set_block(1000, 1000, 1000, far_block.clone()));
        assert_eq!(schematic.get_block(1000, 1000, 1000), Some(&far_block));

        let main_region = schematic.get_region("Main").unwrap();
        assert_eq!(main_region.position, (1000, 1000, 1000));
        assert_eq!(main_region.size, (1, 1, 1));

        // Test that blocks outside the region are not present
        assert_eq!(schematic.get_block(999, 1000, 1000), None);
        // Since the schematic region scaling scales by a factor of 1.5, we need to check 2 blocks away (we apply a ceil)  since we expanded the region (previously 1x1x1)
        assert_eq!(schematic.get_block(1002, 1000, 1000), None);
    }

    #[test]
    fn test_schematic_region_expansion() {
        let mut schematic = UniversalSchematic::new("Expanding Schematic".to_string());

        let block1 = BlockState::new("minecraft:stone".to_string());
        let block2 = BlockState::new("minecraft:dirt".to_string());

        assert!(schematic.set_block(0, 0, 0, block1.clone()));
        assert!(schematic.set_block(10, 20, 30, block2.clone()));

        let main_region = schematic.get_region("Main").unwrap();
        assert_eq!(main_region.position, (0, 0, 0));

        assert_eq!(schematic.get_block(0, 0, 0), Some(&block1));
        assert_eq!(schematic.get_block(10, 20, 30), Some(&block2));
        assert_eq!(schematic.get_block(5, 10, 15), Some(&BlockState::new("minecraft:air".to_string())));
    }

    #[test]
    fn test_schematic_negative_coordinates() {
        let mut schematic = UniversalSchematic::new("Negative Coordinates Schematic".to_string());

        let neg_block = BlockState::new("minecraft:emerald_block".to_string());
        assert!(schematic.set_block(-10, -10, -10, neg_block.clone()));
        assert_eq!(schematic.get_block(-10, -10, -10), Some(&neg_block));

        let main_region = schematic.get_region("Main").unwrap();
        assert!(main_region.position.0 <= -10 && main_region.position.1 <= -10 && main_region.position.2 <= -10);
    }



    #[test]
    fn test_entity_operations() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        let entity = Entity::new("minecraft:creeper".to_string(), (10.5, 65.0, 20.5))
            .with_nbt_data("Fuse".to_string(), "30".to_string());

        assert!(schematic.add_entity(entity.clone()));

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.entities.len(), 1);
        assert_eq!(region.entities[0], entity);

        let removed_entity = schematic.remove_entity( 0).unwrap();
        assert_eq!(removed_entity, entity);

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.entities.len(), 0);
    }

    #[test]
    fn test_block_entity_operations() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (5, 10, 15))
            .with_nbt_data("Items".to_string(), "[{id:\"minecraft:diamond\",Count:64b,Slot:0b}]".to_string());

        assert!(schematic.add_block_entity( block_entity.clone()));

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.block_entities.len(), 1);
        assert_eq!(region.block_entities.get(&(5, 10, 15)), Some(&block_entity));

        let removed_block_entity = schematic.remove_block_entity((5, 10, 15)).unwrap();
        assert_eq!(removed_block_entity, block_entity);

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.block_entities.len(), 0);
    }

    #[test]
    fn test_block_entity_helper_operations() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        // Create a chest block entity with a diamond in slot 0
        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (5, 10, 15))
            .with_item(0, "minecraft:diamond", 64)
            .with_custom_data("Lock", "SecretKey");

        assert!(schematic.add_block_entity(block_entity.clone()));

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.block_entities.len(), 1);
        assert_eq!(region.block_entities.get(&(5, 10, 15)), Some(&block_entity));

        let removed_block_entity = schematic.remove_block_entity((5, 10, 15)).unwrap();
        assert_eq!(removed_block_entity, block_entity);

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.block_entities.len(), 0);
    }

    #[test]
    fn test_block_entity_in_region_operations() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        let block_entity = BlockEntity::new("minecraft:chest".to_string(), (5, 10, 15))
            .with_nbt_data("Items".to_string(), "[{id:\"minecraft:diamond\",Count:64b,Slot:0b}]".to_string());

        assert!(schematic.add_block_entity_in_region("Main", block_entity.clone()));

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.block_entities.len(), 1);
        assert_eq!(region.block_entities.get(&(5, 10, 15)), Some(&block_entity));

        let removed_block_entity = schematic.remove_block_entity_in_region("Main", (5, 10, 15)).unwrap();
        assert_eq!(removed_block_entity, block_entity);

        let region = schematic.get_region("Main").unwrap();
        assert_eq!(region.block_entities.len(), 0);
    }

    #[test]
    fn test_region_palette_operations() {
        let mut region = Region::new("Test".to_string(), (0, 0, 0), (2, 2, 2));

        let stone = BlockState::new("minecraft:stone".to_string());
        let dirt = BlockState::new("minecraft:dirt".to_string());

        region.set_block(0, 0, 0, stone.clone());
        region.set_block(0, 1, 0, dirt.clone());
        region.set_block(1, 0, 0, stone.clone());

        assert_eq!(region.get_block(0, 0, 0), Some(&stone));
        assert_eq!(region.get_block(0, 1, 0), Some(&dirt));
        assert_eq!(region.get_block(1, 0, 0), Some(&stone));
        assert_eq!(region.get_block(1, 1, 1), Some(&BlockState::new("minecraft:air".to_string())));

        // Check the palette size
        assert_eq!(region.palette.len(), 3); // air, stone, dirt
    }

    #[test]
    fn test_nbt_serialization_deserialization() {
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        // Add some blocks and entities
        schematic.set_block(0, 0, 0, BlockState::new("minecraft:stone".to_string()));
        schematic.set_block(1, 1, 1, BlockState::new("minecraft:dirt".to_string()));
        schematic.add_entity(Entity::new("minecraft:creeper".to_string(), (0.5, 0.0, 0.5)));

        // Serialize to NBT
        let nbt = schematic.to_nbt();

        // Write NBT to a buffer
        let mut buffer = Vec::new();
        write_nbt(&mut buffer, None, &nbt, quartz_nbt::io::Flavor::Uncompressed).unwrap();

        // Read NBT from the buffer
        let (read_nbt, _) = read_nbt(&mut Cursor::new(buffer), quartz_nbt::io::Flavor::Uncompressed).unwrap();

        // Deserialize from NBT
        let deserialized_schematic = UniversalSchematic::from_nbt(read_nbt).unwrap();

        // Compare original and deserialized schematics
        assert_eq!(schematic.metadata, deserialized_schematic.metadata);
        assert_eq!(schematic.regions.len(), deserialized_schematic.regions.len());

        // Check if blocks are correctly deserialized
        assert_eq!(schematic.get_block(0, 0, 0), deserialized_schematic.get_block(0, 0, 0));
        assert_eq!(schematic.get_block(1, 1, 1), deserialized_schematic.get_block(1, 1, 1));

        // Check if entities are correctly deserialized
        let original_entities = schematic.get_region("Main").unwrap().entities.clone();
        let deserialized_entities = deserialized_schematic.get_region("Main").unwrap().entities.clone();
        assert_eq!(original_entities, deserialized_entities);

        // Check if palettes are correctly deserialized (now checking the region's palette)
        let original_palette = schematic.get_region("Main").unwrap().palette().clone();
        let deserialized_palette = deserialized_schematic.get_region("Main").unwrap().palette().clone();
        assert_eq!(original_palette, deserialized_palette);
    }



    #[test]
    fn test_multiple_region_merging(){
        let mut schematic = UniversalSchematic::new("Test Schematic".to_string());

        let mut region1 = Region::new("Region1".to_string(), (0, 0, 0), (2, 2, 2));
        let mut region2 = Region::new("Region4".to_string(), (0, 0, 0), (-2, -2, -2));

        // Add some blocks to the regions
        region1.set_block(0, 0, 0, BlockState::new("minecraft:stone".to_string()));
        region1.set_block(1, 1, 1, BlockState::new("minecraft:dirt".to_string()));
        region2.set_block(0, -1, -1, BlockState::new("minecraft:gold_block".to_string()));


        schematic.add_region(region1);
        schematic.add_region(region2);

        let merged_region = schematic.get_merged_region();

        assert_eq!(merged_region.count_blocks(), 3);
        assert_eq!(merged_region.get_block(0, 0, 0), Some(&BlockState::new("minecraft:stone".to_string())));
        assert_eq!(merged_region.get_block(1, 1, 1), Some(&BlockState::new("minecraft:dirt".to_string())));
    }





}
