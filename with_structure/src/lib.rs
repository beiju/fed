mod base_impls;

use std::hash::Hash;
pub use base_impls::*;
pub use perfect_derive; // used in generated macro code
pub use with_structure_derive::WithStructure; // Export the derive macro along with the trait

pub trait WithStructure {
    type Structure: Eq + Hash;

    fn structure(&self) -> Self::Structure;
}
