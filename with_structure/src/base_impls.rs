use crate::WithStructure;
use chrono::{DateTime, Utc};
use std::marker::PhantomData;
use uuid::Uuid;

#[derive(PartialEq, Eq, Hash)]
pub struct MonostateStructure;

macro_rules! trivial_with_structure {
    ($($t:ty),+) => {
        $(impl WithStructure for $t {
            type Structure = MonostateStructure;

            fn structure(&self) -> Self::Structure { MonostateStructure }
        })+
    }
}

trivial_with_structure!(
    (),
    bool,
    f64,
    f32,
    i64,
    i32,
    i16,
    i8,
    isize,
    u64,
    u32,
    u16,
    u8,
    usize,
    Uuid,
    String,
    DateTime<Utc>
);

impl<T> WithStructure for Vec<T> {
    type Structure = MonostateStructure;

    fn structure(&self) -> Self::Structure {
        MonostateStructure
    }
}

impl<T: WithStructure> WithStructure for Option<T> {
    type Structure = Option<T::Structure>;

    fn structure(&self) -> Self::Structure {
        match self {
            None => None,
            Some(inner) => Some(inner.structure()),
        }
    }
}

#[cfg(feature = "either")]
impl<L: WithStructure, R: WithStructure> WithStructure for either::Either<L, R> {
    type Structure = either::Either<L::Structure, R::Structure>;

    fn structure(&self) -> Self::Structure {
        match self {
            either::Either::Left(inner) => either::Either::Left(inner.structure()),
            either::Either::Right(inner) => either::Either::Right(inner.structure()),
        }
    }
}

impl<T: WithStructure> WithStructure for PhantomData<T> {
    type Structure = MonostateStructure;

    fn structure(&self) -> Self::Structure {
        MonostateStructure
    }
}

macro_rules! tuple_impls {
    ( $( $name:ident )+ ) => {
        impl<$($name: WithStructure),+> WithStructure for ($($name),+) {
            type Structure = ($($name::Structure),+);

            // We're reusing the type names as variable names, and rust (arguably correctly)
            // complains about the case
            #[allow(non_snake_case)]
            fn structure(&self) -> Self::Structure {
                let ($($name,)+) = self;
                ($($name.structure(),)+)
            }
        }
    };
}

// The 1-tuple conflicts with vector, not sure why (also not sure that it's vector specifically,
// that may just have been the first conflict rustc noticed)
tuple_impls! { A B }
tuple_impls! { A B C }
tuple_impls! { A B C D }
tuple_impls! { A B C D E }
tuple_impls! { A B C D E F }
tuple_impls! { A B C D E F G }
tuple_impls! { A B C D E F G H }
tuple_impls! { A B C D E F G H I }
tuple_impls! { A B C D E F G H I J }
tuple_impls! { A B C D E F G H I J K }
tuple_impls! { A B C D E F G H I J K L }

macro_rules! array_impls {
    ($n:literal) => {
        impl<T> WithStructure for [T; $n] {
            type Structure = MonostateStructure;
            fn structure(&self) -> Self::Structure {
                MonostateStructure
            }
        }
    };
}

array_impls! { 0 }
array_impls! { 1 }
array_impls! { 2 }
array_impls! { 3 }
array_impls! { 4 }
array_impls! { 5 }
array_impls! { 6 }
array_impls! { 7 }
array_impls! { 8 }
array_impls! { 9 }
array_impls! { 10 }
array_impls! { 11 }
array_impls! { 12 }
