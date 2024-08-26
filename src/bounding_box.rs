use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min: (i32, i32, i32),
    pub max: (i32, i32, i32),
}

impl BoundingBox {
    pub fn new(min: (i32, i32, i32), max: (i32, i32, i32)) -> Self {
        BoundingBox { min, max }
    }

    pub fn contains(&self, point: (i32, i32, i32)) -> bool {
        point.0 >= self.min.0 && point.0 <= self.max.0 &&
            point.1 >= self.min.1 && point.1 <= self.max.1 &&
            point.2 >= self.min.2 && point.2 <= self.max.2
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min.0 <= other.max.0 && self.max.0 >= other.min.0 &&
            self.min.1 <= other.max.1 && self.max.1 >= other.min.1 &&
            self.min.2 <= other.max.2 && self.max.2 >= other.min.2
    }

    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min: (
                self.min.0.min(other.min.0),
                self.min.1.min(other.min.1),
                self.min.2.min(other.min.2),
            ),
            max: (
                self.max.0.max(other.max.0),
                self.max.1.max(other.max.1),
                self.max.2.max(other.max.2),
            ),
        }
    }

    pub fn coords_to_index(&self, x: i32, y: i32, z: i32) -> usize {
        let (width, _, length) = self.get_dimensions();
        let dx = x - self.min.0;
        let dy = y - self.min.1;
        let dz = z - self.min.2;
        (dx + dz * width + dy * width * length) as usize
    }

    pub fn index_to_coords(&self, index: usize) -> (i32, i32, i32) {
        let (width, _, length) = self.get_dimensions();
        let dx = (index % width as usize) as i32;
        let dy = (index / (width * length) as usize) as i32;
        let dz = ((index / width as usize) % length as usize) as i32;
        (dx + self.min.0, dy + self.min.1, dz + self.min.2)
    }

    pub fn get_dimensions(&self) -> (i32, i32, i32) {
        (
            (self.max.0 - self.min.0 + 1),
            (self.max.1 - self.min.1 + 1),
            (self.max.2 - self.min.2 + 1),
        )
    }

    pub fn to_position_and_size(&self) -> ((i32, i32, i32), (i32, i32, i32)) {
        (self.min, self.get_dimensions())
    }

    pub fn from_position_and_size(position: (i32, i32, i32), size: (i32, i32, i32)) -> Self {
        let position2 = (position.0 + size.0, position.1 + size.1, position.2 + size.2);

        let offset_min = (
            -size.0.signum().min(0),
            -size.1.signum().min(0),
            -size.2.signum().min(0),
        );
        let offset_max = (
            -size.0.signum().max(0),
            -size.1.signum().max(0),
            -size.2.signum().max(0),
        );

        BoundingBox::new(
            (position.0.min(position2.0) + offset_min.0, position.1.min(position2.1) + offset_min.1, position.2.min(position2.2) + offset_min.2),
            (position.0.max(position2.0) + offset_max.0, position.1.max(position2.1) + offset_max.1, position.2.max(position2.2) + offset_max.2),

        )
    }
    pub fn volume(&self) -> u64 {
        let (width, height, length) = self.get_dimensions();
        width as u64 * height as u64 * length as u64
    }
}