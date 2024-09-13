use crate::utils::{NbtMap, NbtValue};

#[derive(Clone, Debug, PartialEq)]
pub struct ItemStack {
    pub id: String,
    pub count: u8,
    pub slot: Option<u8>,
}

impl ItemStack {
    pub fn new(id: &str, count: u8) -> Self {
        ItemStack {
            id: id.to_string(),
            count,
            slot: None,
        }
    }

    pub fn with_slot(mut self, slot: u8) -> Self {
        self.slot = Some(slot);
        self
    }

    pub fn to_nbt(&self) -> NbtValue {
        let mut compound = NbtMap::new();
        compound.insert("id".to_string(), NbtValue::String(self.id.clone()));
        compound.insert("Count".to_string(), NbtValue::Byte(self.count as i8));
        if let Some(slot) = self.slot {
            compound.insert("Slot".to_string(), NbtValue::Byte(slot as i8));
        }
        NbtValue::Compound(compound)
    }
}