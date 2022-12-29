pub trait EnumFlatten {
    type Flattened: EnumFlattened;

    fn flatten(&self) -> Self::Flattened;
}

pub trait EnumFlattened {
    type Unflattened: EnumFlatten;

    fn unflatten(&self) -> Self::Unflattened;
}
