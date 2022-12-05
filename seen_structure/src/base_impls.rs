use crate::{HasStructure, ItemStructure};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(PartialEq, Eq, Hash)]
pub struct MonostateStructure;
impl ItemStructure for MonostateStructure {}

macro_rules! trivial_has_structure {
    ($($t:ty),+) => {
        $(impl HasStructure for $t {
            type Structure = MonostateStructure;

            fn structure(&self) -> Self::Structure { MonostateStructure }
        })+
    }
}

trivial_has_structure!(bool, f64, f32, i64, i32, i16, i8, isize, u64, u32, u16, u8, usize, Uuid, String, DateTime<Utc>);