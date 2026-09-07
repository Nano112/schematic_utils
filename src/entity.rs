use quartz_nbt::{NbtCompound, NbtList, NbtTag};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NbtValue {
    String(String),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Byte(i8),
    Short(i16),
    Boolean(bool),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
    ByteArray(Vec<i8>),
    List(Vec<NbtValue>),
    Compound(HashMap<String, NbtValue>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmorStandEquipment {
    pub helmet: Option<String>,
    pub chestplate: Option<String>,
    pub leggings: Option<String>,
    pub boots: Option<String>,
}

impl ArmorStandEquipment {
    /// Create a complete armor set from `diamond`, `netherite`, `iron`, etc.
    pub fn full_set(material: &str) -> Self {
        let material = material.strip_prefix("minecraft:").unwrap_or(material);
        let item = |piece: &str| Some(format!("minecraft:{material}_{piece}"));
        Self {
            helmet: item("helmet"),
            chestplate: item("chestplate"),
            leggings: item("leggings"),
            boots: item("boots"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub position: (f64, f64, f64),
    pub nbt: HashMap<String, NbtValue>,
}

impl Entity {
    pub fn new(id: String, position: (f64, f64, f64)) -> Self {
        Entity {
            id,
            position,
            nbt: HashMap::new(),
        }
    }

    /// Construct an armor stand with typed equipment instead of hand-authored NBT.
    /// Equipment is stored in Minecraft's boots-to-helmet `ArmorItems` order.
    pub fn armor_stand(
        position: (f64, f64, f64),
        yaw: f32,
        equipment: ArmorStandEquipment,
    ) -> Self {
        let item = |id: Option<String>| {
            let mut compound = HashMap::new();
            if let Some(id) = id {
                compound.insert("id".to_string(), NbtValue::String(id));
                compound.insert("Count".to_string(), NbtValue::Byte(1));
            }
            NbtValue::Compound(compound)
        };
        let mut entity = Self::new("minecraft:armor_stand".to_string(), position);
        entity.nbt.insert(
            "Rotation".to_string(),
            NbtValue::List(vec![NbtValue::Float(yaw), NbtValue::Float(0.0)]),
        );
        entity.nbt.insert("ShowArms".to_string(), NbtValue::Byte(1));
        entity.nbt.insert(
            "ArmorItems".to_string(),
            NbtValue::List(vec![
                item(equipment.boots),
                item(equipment.leggings),
                item(equipment.chestplate),
                item(equipment.helmet),
            ]),
        );
        entity
    }

    pub fn with_nbt_data(mut self, key: String, value: String) -> Self {
        self.nbt.insert(key, NbtValue::String(value));
        self
    }

    fn nbt_tag_to_value(tag: &NbtTag) -> NbtValue {
        match tag {
            NbtTag::String(s) => NbtValue::String(s.clone()),
            NbtTag::Int(i) => NbtValue::Int(*i),
            NbtTag::Long(l) => NbtValue::Long(*l),
            NbtTag::Float(f) => NbtValue::Float(*f),
            NbtTag::Double(d) => NbtValue::Double(*d),
            NbtTag::Byte(b) => NbtValue::Byte(*b),
            NbtTag::Short(s) => NbtValue::Short(*s),
            NbtTag::IntArray(arr) => NbtValue::IntArray(arr.clone()),
            NbtTag::LongArray(arr) => NbtValue::LongArray(arr.clone()),
            NbtTag::ByteArray(arr) => NbtValue::ByteArray(arr.clone()),
            NbtTag::List(list) => {
                let values: Vec<NbtValue> = list.iter().map(Self::nbt_tag_to_value).collect();
                NbtValue::List(values)
            }
            NbtTag::Compound(compound) => {
                let mut map = HashMap::new();
                for (key, value) in compound.inner() {
                    map.insert(key.clone(), Self::nbt_tag_to_value(value));
                }
                NbtValue::Compound(map)
            }
        }
    }

    fn value_to_nbt_tag(value: &NbtValue) -> NbtTag {
        match value {
            NbtValue::String(s) => NbtTag::String(s.clone()),
            NbtValue::Int(i) => NbtTag::Int(*i),
            NbtValue::Long(l) => NbtTag::Long(*l),
            NbtValue::Float(f) => NbtTag::Float(*f),
            NbtValue::Double(d) => NbtTag::Double(*d),
            NbtValue::Byte(b) => NbtTag::Byte(*b),
            NbtValue::Short(s) => NbtTag::Short(*s),
            NbtValue::Boolean(b) => NbtTag::Byte(if *b { 1 } else { 0 }),
            NbtValue::IntArray(arr) => NbtTag::IntArray(arr.clone()),
            NbtValue::LongArray(arr) => NbtTag::LongArray(arr.clone()),
            NbtValue::ByteArray(arr) => NbtTag::ByteArray(arr.clone()),
            NbtValue::List(list) => {
                let tags: Vec<NbtTag> = list.iter().map(Self::value_to_nbt_tag).collect();
                NbtTag::List(NbtList::from(tags))
            }
            NbtValue::Compound(map) => {
                let mut compound = NbtCompound::new();
                for (key, value) in map {
                    compound.insert(key, Self::value_to_nbt_tag(value));
                }
                NbtTag::Compound(compound)
            }
        }
    }

    pub fn to_nbt(&self) -> NbtTag {
        let mut compound = NbtCompound::new();

        // Always store the full minecraft:id format
        let full_id = if self.id.starts_with("minecraft:") {
            self.id.clone()
        } else {
            format!("minecraft:{}", self.id)
        };
        compound.insert("id", NbtTag::String(full_id));

        // Add position
        let pos_list = NbtList::from(vec![
            NbtTag::Double(self.position.0),
            NbtTag::Double(self.position.1),
            NbtTag::Double(self.position.2),
        ]);
        compound.insert("Pos", NbtTag::List(pos_list));

        // Write all NBT fields at top level (Minecraft's native format)
        for (key, value) in &self.nbt {
            compound.insert(key, Self::value_to_nbt_tag(value));
        }

        crate::nbt::canonicalize_compound(&mut compound);
        NbtTag::Compound(compound)
    }

    pub fn from_nbt(nbt: &NbtCompound) -> Result<Self, String> {
        // Handle both id cases, but preserve the minecraft: prefix
        let id = match nbt.get::<_, &str>("id") {
            Ok(id) => id.to_string(),
            Err(_) => match nbt.get::<_, &str>("Id") {
                Ok(id) => id.to_string(),
                Err(e) => return Err(format!("Failed to get Entity id: {}", e)),
            },
        };

        // Don't strip the minecraft: prefix anymore
        let id = if id.starts_with("minecraft:") {
            id
        } else {
            format!("minecraft:{}", id)
        };

        let position = nbt
            .get::<_, &NbtList>("Pos")
            .map_err(|e| format!("Failed to get Entity position: {}", e))?;
        let position = if position.len() == 3 {
            (
                position
                    .get::<f64>(0)
                    .map_err(|e| format!("Failed to get X position: {}", e))?,
                position
                    .get::<f64>(1)
                    .map_err(|e| format!("Failed to get Y position: {}", e))?,
                position
                    .get::<f64>(2)
                    .map_err(|e| format!("Failed to get Z position: {}", e))?,
            )
        } else {
            return Err("Invalid position data".to_string());
        };

        // Get NBT data: first check for legacy "NBT" wrapper, then capture all top-level fields.
        // Minecraft stores entity data (Health, Motion, Rotation, Passengers, etc.) as top-level
        // fields, not in a nested "NBT" compound.
        let mut nbt_map = HashMap::new();
        if let Ok(entity_nbt) = nbt.get::<_, &NbtCompound>("NBT") {
            // Legacy Nucleation format: data wrapped in "NBT" compound
            for (key, value) in entity_nbt.inner() {
                nbt_map.insert(key.clone(), Self::nbt_tag_to_value(value));
            }
        } else {
            // Minecraft native format: all data at top level
            for (key, value) in nbt.inner() {
                match key.as_str() {
                    "id" | "Id" | "Pos" => continue, // Already handled
                    _ => {
                        nbt_map.insert(key.clone(), Self::nbt_tag_to_value(value));
                    }
                }
            }
        }

        Ok(Entity {
            id,
            position,
            nbt: nbt_map,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_entity() {
        let entity = Entity::new("minecraft:creeper".to_string(), (1.0, 2.0, 3.0));
        assert_eq!(entity.id, "minecraft:creeper");
        assert_eq!(entity.position, (1.0, 2.0, 3.0));
        assert!(entity.nbt.is_empty());
    }

    #[test]
    fn test_with_nbt_data() {
        let entity = Entity::new("minecraft:creeper".to_string(), (1.0, 2.0, 3.0))
            .with_nbt_data("CustomName".to_string(), "Bob".to_string());

        assert_eq!(entity.nbt.len(), 1);
        assert_eq!(
            entity.nbt.get("CustomName"),
            Some(&NbtValue::String("Bob".to_string()))
        );
    }

    #[test]
    fn test_entity_serialization() {
        let mut entity = Entity::new("minecraft:creeper".to_string(), (1.0, 2.0, 3.0));
        entity
            .nbt
            .insert("Health".to_string(), NbtValue::Float(20.0));
        entity.nbt.insert(
            "CustomName".to_string(),
            NbtValue::String("Bob".to_string()),
        );

        let nbt = entity.to_nbt();

        if let NbtTag::Compound(compound) = nbt {
            assert_eq!(compound.get::<_, &str>("id").unwrap(), "minecraft:creeper");

            let pos = compound.get::<_, &NbtList>("Pos").unwrap();
            assert_eq!(pos.get::<f64>(0).unwrap(), 1.0);
            assert_eq!(pos.get::<f64>(1).unwrap(), 2.0);
            assert_eq!(pos.get::<f64>(2).unwrap(), 3.0);

            // Fields are at top level (Minecraft native format)
            assert_eq!(compound.get::<_, f32>("Health").unwrap(), 20.0);
            assert_eq!(compound.get::<_, &str>("CustomName").unwrap(), "Bob");
        } else {
            panic!("Expected Compound NBT tag");
        }
    }

    #[test]
    fn test_entity_deserialization_legacy_nbt_wrapper() {
        // Test legacy format with "NBT" wrapper compound
        let mut compound = NbtCompound::new();
        compound.insert("id", NbtTag::String("minecraft:creeper".to_string()));

        let pos_list = NbtList::from(vec![
            NbtTag::Double(1.0),
            NbtTag::Double(2.0),
            NbtTag::Double(3.0),
        ]);
        compound.insert("Pos", NbtTag::List(pos_list));

        let mut nbt_data = NbtCompound::new();
        nbt_data.insert("Health", NbtTag::Float(20.0));
        nbt_data.insert("CustomName", NbtTag::String("Bob".to_string()));
        compound.insert("NBT", NbtTag::Compound(nbt_data));

        let entity = Entity::from_nbt(&compound).unwrap();

        assert_eq!(entity.id, "minecraft:creeper");
        assert_eq!(entity.position, (1.0, 2.0, 3.0));
        assert_eq!(entity.nbt.get("Health"), Some(&NbtValue::Float(20.0)));
        assert_eq!(
            entity.nbt.get("CustomName"),
            Some(&NbtValue::String("Bob".to_string()))
        );
    }

    #[test]
    fn test_entity_deserialization_minecraft_native() {
        // Test Minecraft native format with top-level fields
        let mut compound = NbtCompound::new();
        compound.insert("id", NbtTag::String("minecraft:creeper".to_string()));

        let pos_list = NbtList::from(vec![
            NbtTag::Double(1.0),
            NbtTag::Double(2.0),
            NbtTag::Double(3.0),
        ]);
        compound.insert("Pos", NbtTag::List(pos_list));

        // Top-level fields (how Minecraft actually stores entity data)
        compound.insert("Health", NbtTag::Float(20.0));
        compound.insert("CustomName", NbtTag::String("Bob".to_string()));
        compound.insert("Fire", NbtTag::Short(-1));
        compound.insert("OnGround", NbtTag::Byte(1));

        let motion = NbtList::from(vec![
            NbtTag::Double(0.0),
            NbtTag::Double(-0.078),
            NbtTag::Double(0.0),
        ]);
        compound.insert("Motion", NbtTag::List(motion));

        let rotation = NbtList::from(vec![NbtTag::Float(90.0), NbtTag::Float(0.0)]);
        compound.insert("Rotation", NbtTag::List(rotation));

        let entity = Entity::from_nbt(&compound).unwrap();

        assert_eq!(entity.id, "minecraft:creeper");
        assert_eq!(entity.position, (1.0, 2.0, 3.0));
        assert_eq!(entity.nbt.get("Health"), Some(&NbtValue::Float(20.0)));
        assert_eq!(
            entity.nbt.get("CustomName"),
            Some(&NbtValue::String("Bob".to_string()))
        );
        assert_eq!(entity.nbt.get("Fire"), Some(&NbtValue::Short(-1)));
        assert_eq!(entity.nbt.get("OnGround"), Some(&NbtValue::Byte(1)));
        assert!(entity.nbt.contains_key("Motion"));
        assert!(entity.nbt.contains_key("Rotation"));
    }

    #[test]
    fn test_entity_deserialization_with_passengers() {
        // Test entity with Passengers (riding)
        let mut compound = NbtCompound::new();
        compound.insert("id", NbtTag::String("minecraft:pig".to_string()));
        compound.insert(
            "Pos",
            NbtTag::List(NbtList::from(vec![
                NbtTag::Double(10.0),
                NbtTag::Double(64.0),
                NbtTag::Double(20.0),
            ])),
        );
        compound.insert("Health", NbtTag::Float(10.0));

        // Add a passenger (riding entity)
        let mut passenger = NbtCompound::new();
        passenger.insert("id", NbtTag::String("minecraft:zombie".to_string()));
        passenger.insert(
            "Pos",
            NbtTag::List(NbtList::from(vec![
                NbtTag::Double(10.0),
                NbtTag::Double(65.0),
                NbtTag::Double(20.0),
            ])),
        );
        passenger.insert("Health", NbtTag::Float(20.0));

        let passengers = NbtList::from(vec![NbtTag::Compound(passenger)]);
        compound.insert("Passengers", NbtTag::List(passengers));

        let entity = Entity::from_nbt(&compound).unwrap();
        assert_eq!(entity.id, "minecraft:pig");
        assert!(entity.nbt.contains_key("Passengers"));

        // Verify passengers data is preserved
        if let Some(NbtValue::List(passengers)) = entity.nbt.get("Passengers") {
            assert_eq!(passengers.len(), 1);
            if let NbtValue::Compound(p) = &passengers[0] {
                assert_eq!(
                    p.get("id"),
                    Some(&NbtValue::String("minecraft:zombie".to_string()))
                );
            } else {
                panic!("Expected compound in passengers list");
            }
        } else {
            panic!("Expected Passengers list");
        }
    }

    #[test]
    fn test_complex_nbt_values() {
        let mut entity = Entity::new("minecraft:item".to_string(), (0.0, 0.0, 0.0));

        // Test array types
        entity
            .nbt
            .insert("IntArray".to_string(), NbtValue::IntArray(vec![1, 2, 3]));
        entity
            .nbt
            .insert("LongArray".to_string(), NbtValue::LongArray(vec![1, 2, 3]));
        entity
            .nbt
            .insert("ByteArray".to_string(), NbtValue::ByteArray(vec![1, 2, 3]));

        // Test nested compound
        let mut nested_map = HashMap::new();
        nested_map.insert(
            "NestedString".to_string(),
            NbtValue::String("test".to_string()),
        );
        entity
            .nbt
            .insert("NestedCompound".to_string(), NbtValue::Compound(nested_map));

        // Test list
        entity.nbt.insert(
            "Tags".to_string(),
            NbtValue::List(vec![
                NbtValue::String("a".to_string()),
                NbtValue::String("b".to_string()),
            ]),
        );

        let nbt = entity.to_nbt();
        if let NbtTag::Compound(compound) = nbt {
            let deserialized = Entity::from_nbt(&compound).unwrap();
            assert_eq!(entity, deserialized);
        } else {
            panic!("Expected Compound NBT tag");
        }
    }

    #[test]
    fn test_id_prefix_handling() {
        // Test with minecraft: prefix
        let entity1 = Entity::new("minecraft:creeper".to_string(), (0.0, 0.0, 0.0));
        let nbt1 = entity1.to_nbt();
        if let NbtTag::Compound(compound) = nbt1 {
            let deserialized1 = Entity::from_nbt(&compound).unwrap();
            assert_eq!(deserialized1.id, "minecraft:creeper");
        } else {
            panic!("Expected Compound NBT tag");
        }

        // Test without minecraft: prefix
        let entity2 = Entity::new("creeper".to_string(), (0.0, 0.0, 0.0));
        let nbt2 = entity2.to_nbt();
        if let NbtTag::Compound(compound) = nbt2 {
            let deserialized2 = Entity::from_nbt(&compound).unwrap();
            assert_eq!(deserialized2.id, "minecraft:creeper");
        } else {
            panic!("Expected Compound NBT tag");
        }
    }

    #[test]
    fn armor_stand_equipment_full_set_expands_a_material_name() {
        let equipment = ArmorStandEquipment::full_set("diamond");

        assert_eq!(
            equipment.helmet.as_deref(),
            Some("minecraft:diamond_helmet")
        );
        assert_eq!(
            equipment.chestplate.as_deref(),
            Some("minecraft:diamond_chestplate")
        );
        assert_eq!(
            equipment.leggings.as_deref(),
            Some("minecraft:diamond_leggings")
        );
        assert_eq!(equipment.boots.as_deref(), Some("minecraft:diamond_boots"));
    }

    #[test]
    fn armor_stand_constructor_builds_typed_equipment_and_rotation_nbt() {
        let entity = Entity::armor_stand(
            (0.5, 1.0, 0.5),
            180.0,
            ArmorStandEquipment {
                helmet: Some("minecraft:diamond_helmet".to_string()),
                chestplate: Some("minecraft:diamond_chestplate".to_string()),
                leggings: Some("minecraft:diamond_leggings".to_string()),
                boots: Some("minecraft:diamond_boots".to_string()),
            },
        );

        assert_eq!(entity.id, "minecraft:armor_stand");
        assert_eq!(
            entity.nbt.get("Rotation"),
            Some(&NbtValue::List(vec![
                NbtValue::Float(180.0),
                NbtValue::Float(0.0),
            ]))
        );
        let NbtValue::List(items) = entity.nbt.get("ArmorItems").unwrap() else {
            panic!("ArmorItems should be a list");
        };
        assert_eq!(items.len(), 4);
        for (item, suffix) in items
            .iter()
            .zip(["boots", "leggings", "chestplate", "helmet"])
        {
            let NbtValue::Compound(item) = item else {
                panic!("armor item should be a compound");
            };
            assert_eq!(
                item.get("id"),
                Some(&NbtValue::String(format!("minecraft:diamond_{suffix}")))
            );
            assert_eq!(item.get("Count"), Some(&NbtValue::Byte(1)));
        }
    }

    #[test]
    fn test_invalid_nbt() {
        // Test missing id
        let mut compound = NbtCompound::new();
        compound.insert(
            "Pos",
            NbtTag::List(NbtList::from(vec![
                NbtTag::Double(0.0),
                NbtTag::Double(0.0),
                NbtTag::Double(0.0),
            ])),
        );
        assert!(Entity::from_nbt(&compound).is_err());

        // Test invalid position
        let mut compound = NbtCompound::new();
        compound.insert("id", NbtTag::String("minecraft:creeper".to_string()));
        compound.insert(
            "Pos",
            NbtTag::List(NbtList::from(vec![
                NbtTag::Double(0.0),
                NbtTag::Double(0.0),
            ])),
        );
        assert!(Entity::from_nbt(&compound).is_err());
    }
}
