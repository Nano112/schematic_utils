use quartz_nbt::{NbtCompound, NbtTag};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub name: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub created: Option<u64>,
    pub modified: Option<u64>,
    pub lm_version: Option<i32>,
    pub mc_version: Option<i32>,
    pub we_version: Option<i32>,
}
impl Default for Metadata {
    fn default() -> Self {
        Metadata {
            name: None,
            author: None,
            description: None,
            created: None,
            modified: None,
            lm_version: None,
            mc_version: None,
            we_version: None,
        }
    }
}

impl Metadata {
    pub fn new(
        name: Option<String>,
        author: Option<String>,
        description: Option<String>,
        created: Option<u64>,
        modified: Option<u64>,
        lm_version: Option<i32>,
        mc_version: Option<i32>,
        we_version: Option<i32>,
    ) -> Self {
        Metadata {
            name,
            author,
            description,
            created,
            modified,
            lm_version,
            mc_version,
            we_version,
        }
    }

    pub fn to_nbt(&self) -> NbtTag {
        let mut compound = NbtCompound::new();

        if let Some(name) = &self.name {
            compound.insert("Name", NbtTag::String(name.clone()));
        }
        if let Some(author) = &self.author {
            compound.insert("Author", NbtTag::String(author.clone()));
        }
        if let Some(description) = &self.description {
            compound.insert("Description", NbtTag::String(description.clone()));
        }
        if let Some(created) = self.created {
            compound.insert("TimeCreated", NbtTag::Long(created as i64));
        }
        if let Some(modified) = self.modified {
            compound.insert("TimeModified", NbtTag::Long(modified as i64));
        }
        if let Some(lm_version) = self.lm_version {
            compound.insert("lm_version", NbtTag::Int(lm_version));
        }
        if let Some(mc_version) = self.mc_version {
            compound.insert("mc_version", NbtTag::Int(mc_version));
        }
        if let Some(we_version) = self.we_version {
            compound.insert("we_version", NbtTag::Int(we_version));
        }

        NbtTag::Compound(compound)
    }

    pub fn from_nbt(nbt: &NbtCompound) -> Result<Self, String> {
        let name = nbt.get::<_, &str>("Name").map_err(|_| "").ok().map(|s| s.to_string());
        let author = nbt.get::<_, &str>("Author").map_err(|_| "").ok().map(|s| s.to_string());
        let description = nbt.get::<_, &str>("Description").map_err(|_| "").ok().map(|s| s.to_string());
        let created = nbt.get::<_, i64>("TimeCreated").map_err(|_| 0).ok().map(|v| v as u64);
        let modified = nbt.get::<_, i64>("TimeModified").map_err(|_| 0).ok().map(|v| v as u64);
        let lm_version = nbt.get::<_, i32>("lm_version").map_err(|_| 0).ok();
        let mc_version = nbt.get::<_, i32>("mc_version").map_err(|_| 0).ok();
        let we_version = nbt.get::<_, i32>("we_version").map_err(|_| 0).ok();

        Ok(Metadata::new(name, author, description, created, modified, lm_version, mc_version, we_version))
    }
}
