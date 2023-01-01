#![feature(let_chains)]

use ::proc_macro::TokenStream;
use std::collections::HashMap;
use ::proc_macro2::{TokenStream as TokenStream2};
use syn::{parse_macro_input, DeriveInput, Data, DataStruct};
use quote::{quote, ToTokens};
use ::syn::{*, Result};
use itertools::Itertools;
use macro_state::{proc_init_state, proc_write_state};
use proc_macro2::Span;
use serde::{Deserialize, Serialize};

const PROPAGATED_ATTRS: [&str; 1] = ["doc"];

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
            Type::Path(TypePath { qself, path }) if qself.is_none() => {
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

#[derive(Serialize, Deserialize, Debug)]
struct SerializableAttribute {
    path: String,
    tokens: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct StructField {
    attrs: Vec<SerializableAttribute>,
    vis: String,
    name: String,
    ty: String,
}

impl StructField {
    pub fn to_field_with_attrs(&self, allowed_attrs: &[&str]) -> Field {
        self.to_field_with_attrs_vis(allowed_attrs, None)
    }

    pub fn to_field_with_attrs_vis(&self, allowed_attrs: &[&str], force_visible: Option<&Visibility>) -> Field {
        Field {
            attrs: self.attrs.iter()
                .filter_map(|attr| {
                    if allowed_attrs.iter().any(|&a| a == &attr.path) {
                        Some(Attribute {
                            pound_token: Default::default(),
                            style: AttrStyle::Outer,
                            bracket_token: Default::default(),
                            path: parse_str(&attr.path).expect("Error parsing attribute path"),
                            tokens: parse_str(&attr.tokens).expect("Error parsing attribute tokens"),
                        })
                    } else {
                        None
                    }
                })
                .collect(),
            vis: force_visible.cloned().unwrap_or_else(|| {
                parse_str(&self.vis).expect("Error parsing field vis")
            }),
            ident: parse_str(&self.name).expect("Error parsing field ident"),
            colon_token: Some(Default::default()),
            ty: parse_str(&self.ty).expect("Error parsing field type"),
        }
    }
}

impl From<&Field> for StructField {
    fn from(field: &Field) -> Self {
        Self {
            attrs: field.attrs.iter()
                .map(|attr| {
                    SerializableAttribute {
                        path: attr.path.to_token_stream().to_string(),
                        tokens: attr.tokens.to_string(),
                    }
                })
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
    flatten_item_vis: String,
    flatten_item_name: String,
    flatten_item_type: String,
    fields: Vec<StructField>,
}

fn find_stored_enum(ty: &Type) -> Option<FlattenableEnumMeta> {
    let mut map = get_map("enum_flatten_derive_enums");
    // Using remove because I want to move out of the map. It doesn't matter because it will be
    // discarded immediately after
    map.remove(&ty.into())
}

fn get_map<T: Serialize + for<'de> Deserialize<'de>>(key: &str) -> HashMap<SerializableIdent, T> {
    let empty_map = HashMap::<SerializableIdent, T>::new();
    let empty_map_str = serde_json::to_string(&empty_map)
        .expect("Error stringifying empty map");
    let map_str = proc_init_state(key, &empty_map_str)
        .expect("Internal error: Reading map from disk failed");
    serde_json::from_str(&map_str)
        .expect("Internal error: Map structure on disk is invalid")
}

fn store_struct(ty: &Type, st: FlattenEnumMeta) {
    let mut map = get_map("enum_flatten_derive_enums");

    // Using remove because I want to move out of the map. It doesn't matter because it will be
    // discarded immediately after
    map.insert(ty.into(), st);

    let save_str = serde_json::to_string(&map)
        .expect("Internal error: Map structure on disk is invalid");
    proc_write_state("enum_flatten_derive_structs", &save_str)
        .expect("Internal error: Failed to save struct");
}

fn impl_enum_flatten_for_struct(item_vis: Visibility, name: Ident, s: DataStruct, flatten_field_name: Ident) -> Result<TokenStream2> {
    let (flatten_field_vis, flatten_field_type) = match &s.fields {
        Fields::Named(fields) => {
            fields.named.iter()
                .filter_map(|field| {
                    let field_name = field.ident.as_ref()
                        .expect("I think this should always exist within a struct with named fields");
                    if field_name == &flatten_field_name {
                        Some((&field.vis, &field.ty))
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
        .filter_map(|field| {
            if let Some(ident) = &field.ident && ident != &flatten_field_name {
                Some(field.into())
            } else {
                None
            }
        })
        .collect();

    let st = FlattenEnumMeta {
        item_vis: item_vis.to_token_stream().to_string(),
        name: name.to_string(),
        flatten_item_vis: flatten_field_vis.to_token_stream().to_string(),
        flatten_item_name: flatten_field_name.to_string(),
        flatten_item_type: flatten_field_type.to_token_stream().to_string(),
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
    let mut map = get_map("enum_flatten_derive_structs");

    // Using remove because I want to move out of the map. It doesn't matter because it will be
    // discarded immediately after
    map.remove(&ident.into())
}

fn store_enum(ident: &Ident, st: FlattenableEnumMeta) {
    let mut map = get_map("enum_flatten_derive_enums");

    map.insert(ident.into(), st);

    let save_str = serde_json::to_string(&map)
        .expect("Internal error: Map structure is invalid");
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
    let struct_vis: Visibility = parse_str(&st.item_vis).expect("Error parsing struct_vis");
    let struct_name: Ident = parse_str(&st.name).expect("Error parsing struct_name");
    let flattened_field_vis: Visibility = parse_str(&st.flatten_item_vis).expect("Error parsing flatten item vis");
    let flattened_field_name: Ident = parse_str(&st.flatten_item_name).expect("Error parsing flatten item name");
    let flattened_field_type: Type = parse_str(&st.flatten_item_type).expect("Error parsing flatten item type");
    let flat_enum_name = Ident::new(&format!("{}Flat", st.name), Span::call_site());
    let common_fields: Vec<_> = st.fields.iter()
        .map(|field| field.to_field_with_attrs(&PROPAGATED_ATTRS))
        .collect();
    let common_field_names: Vec<Ident> = st.fields.iter()
        .map(|field| parse_str(&field.name).expect("Error parsing common field name"))
        .collect();
    let child_structs: Vec<_> = en.variants.iter()
        .map(|variant| {
            let variant_name: Ident = parse_str(&variant.name).expect("Error parsing variant name");
            let child_struct_name = Ident::new(&format!("{}{}", st.name, variant.name), Span::call_site());
            let child_fields: Vec<_> = variant.fields.iter()
                .map(|field| field.to_field_with_attrs_vis(&PROPAGATED_ATTRS, Some(&flattened_field_vis)))
                .collect();

            quote! {
                // TODO: Let user specify derives for this
                #[derive(Debug, Clone)]
                #struct_vis struct #child_struct_name {
                    #(#common_fields,)*
                    #(#child_fields,)*
                }

                impl Into<#flat_enum_name> for #child_struct_name {
                    fn into(self) -> #flat_enum_name {
                        #flat_enum_name::#variant_name(self)
                    }
                }
            }
        })
        .collect();

    let flat_enum_variants: Vec<_> = en.variants.iter()
        .map(|variant| {
            let variant_name: Ident = parse_str(&variant.name).expect("Error parsing variant name");
            let child_struct_name = Ident::new(&format!("{}{}", st.name, variant.name), Span::call_site());

            quote! {
                #variant_name(#child_struct_name)
            }
        })
        .collect();

    let flatten_match_arms: Vec<_> = en.variants.iter()
        .map(|variant| {
            let variant_name: Ident = parse_str(&variant.name).expect("Error parsing variant name");
            let child_struct_name = Ident::new(&format!("{}{}", st.name, variant.name), Span::call_site());
            let child_field_names: Vec<Ident> = variant.fields.iter()
                .map(|field| {
                    parse_str(&field.name).expect("Error parsing field name")
                })
                .collect();

            quote! {
                #flattened_field_type::#variant_name { #(#child_field_names,)* } => {
                    #child_struct_name {
                        #(#common_field_names: self.#common_field_names,)*
                        #(#child_field_names,)*
                    }.into()
                }
            }
        })
        .collect();

    let unflatten_match_arms: Vec<_> = en.variants.iter()
        .map(|variant| {
            let variant_name: Ident = parse_str(&variant.name).expect("Error parsing variant name");
            let child_field_names: Vec<Ident> = variant.fields.iter()
                .map(|field| {
                    parse_str(&field.name).expect("Error parsing field name")
                })
                .collect();

            quote! {
                #flat_enum_name::#variant_name(inner) => {
                    #struct_name {
                        #(#common_field_names: inner.#common_field_names,)*
                        #flattened_field_name: #flattened_field_type::#variant_name {
                            #(#child_field_names: inner.#child_field_names,)*
                        }
                    }
                }
            }
        })
        .collect();

    Ok({
        quote! {
            #(#child_structs)*

            // TODO: Let user specify derives for this
            #[derive(Debug, Clone)]
            #struct_vis enum #flat_enum_name {
                #(#flat_enum_variants,)*
            }

            impl ::enum_flatten::EnumFlatten for #struct_name {
                type Flattened = #flat_enum_name;

                fn flatten(self) -> Self::Flattened {
                    match self.#flattened_field_name {
                        #(#flatten_match_arms)*
                    }
                }
            }

            impl Into<#flat_enum_name> for #struct_name {
                fn into(self) -> #flat_enum_name {
                    ::enum_flatten::EnumFlatten::flatten(self)
                }
            }

            pub struct ThisIsADebugStructToGrepFor2;

            impl ::enum_flatten::EnumFlattened for #flat_enum_name {
                type Unflattened = #struct_name;

                fn unflatten(self) -> Self::Unflattened {
                    match self {
                        #(#unflatten_match_arms)*
                    }
                }
            }

            impl Into<#struct_name> for #flat_enum_name {
                fn into(self) -> #struct_name {
                    ::enum_flatten::EnumFlattened::unflatten(self)
                }
            }
        }
    })
}