use crate::{WithStructure, ItemStructure};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(PartialEq, Eq, Hash)]
pub struct MonostateStructure;
impl ItemStructure for MonostateStructure {}

macro_rules! trivial_with_structure {
    ($($t:ty),+) => {
        $(impl WithStructure for $t {
            type Structure = MonostateStructure;

            fn structure(&self) -> Self::Structure { MonostateStructure }
        })+
    }
}

trivial_with_structure!((), bool, f64, f32, i64, i32, i16, i8, isize, u64, u32, u16, u8, usize, Uuid, String, DateTime<Utc>);

impl<T> WithStructure for Vec<T> {
    type Structure = MonostateStructure;

    fn structure(&self) -> Self::Structure { MonostateStructure }
}

impl<T: ItemStructure> ItemStructure for Option<T> {}

impl<T: WithStructure> WithStructure for Option<T> {
    type Structure = Option<T::Structure>;

    fn structure(&self) -> Self::Structure {
        match self {
            None => { None }
            Some(inner) => { Some(inner.structure()) }
        }
    }
}