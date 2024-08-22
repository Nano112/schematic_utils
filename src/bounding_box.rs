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

    pub fn get_dimensions(&self) -> (i32, i32, i32) {
        (
            (self.max.0 - self.min.0 + 1),
            (self.max.1 - self.min.1 + 1),
            (self.max.2 - self.min.2 + 1),
        )
    }

    pub fn volume(&self) -> u64 {
        let (width, height, length) = self.get_dimensions();
        width as u64 * height as u64 * length as u64
    }
}