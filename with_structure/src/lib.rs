mod base_impls;

use std::hash::Hash;
pub use base_impls::*;
pub use perfect_derive; // used in generated macro code

pub trait WithStructure {
    type Structure: Eq + Hash;

    fn structure(&self) -> Self::Structure;
}
