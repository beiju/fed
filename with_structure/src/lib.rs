mod base_impls;

pub use base_impls::*;
pub use perfect_derive; // used in generated macro code
use std::hash::Hash;
use serde::Serialize;
pub use with_structure_derive::WithStructure; // Export the derive macro along with the trait

pub trait WithStructure: Serialize {
    // TODO this always conditions on Serialize, even if it's not enabled
    type Structure: Eq + Hash + Serialize;

    fn structure(&self) -> Self::Structure;
}
