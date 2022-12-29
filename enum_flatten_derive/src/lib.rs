use ::proc_macro::TokenStream;
use std::collections::HashMap;
use ::proc_macro2::{TokenStream as TokenStream2};
use syn::{parse_macro_input, DeriveInput, Data, DataStruct};
use quote::{quote, ToTokens};
use ::syn::{*, Result};
use itertools::Itertools;
use macro_state::{has_state, init_state, proc_write_state, read_state};
use proc_macro2::Span;
use serde::{Deserialize, Serialize};

#[proc_macro_derive(EnumFlatten, attributes(enum_flatten))]
pub fn enum_flatten_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as _);
    TokenStream::from(match impl_enum_flatten(ast) {
        | Ok(it) => it,
        | Err(err) => err.to_compile_error(),
    })
}

fn impl_enum_flatten(ast: DeriveInput) -> Result<TokenStream2> {
    let item_vis = ast.vis;
    let name = ast.ident;
    let flatten_field_vec = ast.attrs.iter()
        .filter_map(|attr| {
            if attr.path.is_ident("enum_flatten") {
                Some(attr.parse_args::<syn::Ident>())
            } else {
                None
            }
        })
        .collect::<Result<Vec<_>>>()?;

    // This is purely to get a nice error message. The code would work fine without it.
    if flatten_field_vec.is_empty() {
        panic!("EnumFlatten expects a enum_flatten(name) attribute where `name` is the name of the enum field to flatten");
    }
    let Ok(flatten_field) = flatten_field_vec.into_iter().exactly_one() else {
        panic!("Error: enum_flatten(...) attribute was specified multiple times");
    };

    match ast.data {
        Data::Struct(s) => impl_enum_flatten_for_struct(item_vis, name, s, flatten_field),
        _ => panic!("EnumFlatten may only be used on structs"),
    }
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq)]
struct SerializableIdent(String);

impl From<Ident> for SerializableIdent {
    fn from(value: Ident) -> Self {
        Self(value.to_string())
    }
}

impl From<&Ident> for SerializableIdent {
    fn from(value: &Ident) -> Self {
        Self(value.to_string())
    }
}

impl From<&Type> for SerializableIdent {
    fn from(value: &Type) -> Self {
        Self(match value {
            Type::Path(TypePath { qself: None, path }) => {
                path.segments.last().as_ref()
                    .expect("Type path cannot be empty")
                    .ident
                    .to_string()
            }
            _ => {
                panic!("EnumFlatten may only be used on a field with a simple type")
            }
        })
    }
}

#[derive(Serialize, Deserialize)]
struct StructField {
    attrs: Vec<String>,
    vis: String,
    name: String,
    ty: String,
}

impl StructField {
    pub fn to_token_stream(&self) -> TokenStream2 {
        let attrs = self.attrs.iter()
            .map(|attr| attr.parse().unwrap())
            .collect::<Vec<proc_macro2::TokenStream>>();
        let field_vis: proc_macro2::TokenStream = self.vis.parse().unwrap();
        let field_name: proc_macro2::TokenStream = self.name.parse().unwrap();
        let field_ty: proc_macro2::TokenStream = self.ty.parse().unwrap();
        quote! {
            #(#attrs)*
            #field_vis #field_name: #field_ty
        }
    }
}

impl From<&Field> for StructField {
    fn from(field: &Field) -> Self {
        Self {
            attrs: field.attrs.iter()
                .map(|attr| attr.to_token_stream().to_string())
                .collect(),
            vis: field.vis.to_token_stream().to_string(),
            name: field.ident.to_token_stream().to_string(),
            ty: field.ty.to_token_stream().to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FlattenableEnumVariant {
    name: String,
    fields: Vec<StructField>,
}

#[derive(Serialize, Deserialize)]
struct FlattenableEnumMeta {
    variants: Vec<FlattenableEnumVariant>,
}

#[derive(Serialize, Deserialize)]
struct FlattenEnumMeta {
    item_vis: String,
    name: String,
    fields: Vec<StructField>,
}

fn find_stored_enum(ty: &Type) -> Option<FlattenableEnumMeta> {
    let map_str = init_state!("enum_flatten_derive_enums", "{}");
    let mut map: HashMap<SerializableIdent, FlattenableEnumMeta> = serde_json::from_str(&map_str)
        .expect("Internal error: Map structure on disk is invalid");
    // Using remove because I want to move out of the map. It doesn't matter because it will be
    // discarded immediately after
    map.remove(&ty.into())
}

fn store_struct(ty: &Type, st: FlattenEnumMeta) {
    let map_str = init_state!("enum_flatten_derive_structs", "{}");
    let mut map: HashMap<SerializableIdent, FlattenEnumMeta> = serde_json::from_str(&map_str)
            .expect("Internal error: Map structure on disk is invalid");

    // Using remove because I want to move out of the map. It doesn't matter because it will be
    // discarded immediately after
    map.insert(ty.into(), st);

    let save_str = serde_json::to_string(&map)
        .expect("Internal error: Map structure on disk is invalid");
    proc_write_state("enum_flatten_derive_structs", &save_str)
        .expect("Error saving struct");
}

fn impl_enum_flatten_for_struct(item_vis: Visibility, name: Ident, s: DataStruct, flatten_field_name: Ident) -> Result<TokenStream2> {
    let flatten_field_type = match &s.fields {
        Fields::Named(fields) => {
            fields.named.iter()
                .filter_map(|field| {
                    let field_name = field.ident.as_ref()
                        .expect("I think this should always exist within a struct with named fields");
                    if field_name == &flatten_field_name {
                        Some(&field.ty)
                    } else {
                        None
                    }
                })
                .exactly_one()
                .unwrap_or_else(|e| {
                    panic!("When looking for field {flatten_field_name}, {e}");
                })
        }
        _ => {
            panic!("EnumFlatten can only be used on structs with named fields")
        }
    };

    let fields = s.fields.iter()
        .map(|field| field.into())
        .collect();

    let st = FlattenEnumMeta {
        item_vis: item_vis.to_token_stream().to_string(),
        name: name.to_string(),
        fields,
    };

    if let Some(en) = find_stored_enum(flatten_field_type) {
        generate_enum_flatten_impl(st, en)
    } else {
        store_struct(flatten_field_type, st);
        Ok(quote! {})
    }
}

#[proc_macro_derive(EnumFlattenable)]
pub fn enum_flattenable_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as _);
    TokenStream::from(match impl_enum_flattenable(ast) {
        | Ok(it) => it,
        | Err(err) => err.to_compile_error(),
    })
}

fn impl_enum_flattenable(ast: DeriveInput) -> Result<TokenStream2> {
    match ast.data {
        Data::Enum(e) => impl_enum_flattenable_for_enum(ast.ident, e),
        _ => panic!("EnumFlattenable may only be used on enums"),
    }
}

fn find_stored_struct(ident: &Ident) -> Option<FlattenEnumMeta> {
    if !has_state!("enum_flatten_derive_structs") {
        return None;
    }

    let map_str = read_state!("enum_flatten_derive_structs");
    let mut map: HashMap<SerializableIdent, FlattenEnumMeta> = serde_json::from_str(&map_str)
        .expect("Internal error: Map structure on disk is invalid");
    // Using remove because I want to move out of the map. It doesn't matter because it will be
    // discarded immediately after
    map.remove(&ident.into())
}

fn store_enum(ident: &Ident, st: FlattenableEnumMeta) {
    let mut map: HashMap<SerializableIdent, FlattenableEnumMeta> = if has_state!("enum_flatten_derive_enums") {
        let map_str = read_state!("enum_flatten_derive_enums");
        serde_json::from_str(&map_str)
            .expect("Internal error: Map structure on disk is invalid")
    } else {
        HashMap::new()
    };

    // Using remove because I want to move out of the map. It doesn't matter because it will be
    // discarded immediately after
    map.insert(ident.into(), st);

    let save_str = serde_json::to_string(&map)
        .expect("Internal error: Map structure on disk is invalid");
    proc_write_state("enum_flatten_derive_enums", &save_str)
        .expect("Error saving struct");
}

fn impl_enum_flattenable_for_enum(name: Ident, e: DataEnum) -> Result<TokenStream2> {
    let en = FlattenableEnumMeta {
        variants: e.variants.into_iter()
            .map(|variant| {
                FlattenableEnumVariant {
                    name: variant.ident.to_string(),
                    fields: variant.fields.iter()
                        .map(|field| field.into())
                        .collect(),
                }
            })
            .collect(),
    };

    if let Some(st) = find_stored_struct(&name) {
        generate_enum_flatten_impl(st, en)
    } else {
        store_enum(&name, en);
        Ok(quote! {})
    }
}

fn generate_enum_flatten_impl(st: FlattenEnumMeta, en: FlattenableEnumMeta) -> Result<TokenStream2> {
    let item_vis = st.item_vis;
    let common_fields: Vec<_> = st.fields.into_iter()
        .map(|field| field.to_token_stream())
        .collect();
    let child_structs: Vec<_> = en.variants.into_iter()
        .map(|variant| {
            let child_struct_name = Ident::new(&format!("{}{}", st.name, variant.name), Span::call_site());
            let child_fields: Vec<_> = variant.fields.into_iter()
                .map(|field| field.to_token_stream())
                .collect();

            quote! {
                #item_vis struct #child_struct_name {
                    #(#common_fields),*
                    #(#child_fields),*
                }
            }
        })
        .collect();

    Ok({
        quote! {
             #(#child_structs)*

            // TODO Impls
        }
    })
}