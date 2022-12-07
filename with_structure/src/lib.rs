mod base_impls;

use std::hash::Hash;
pub use base_impls::*;

// Records the structure of a single item (struct or enum)
pub trait ItemStructure: Eq + Hash {

}

pub trait WithStructure {
    type Structure: ItemStructure;

    fn structure(&self) -> Self::Structure;
}
