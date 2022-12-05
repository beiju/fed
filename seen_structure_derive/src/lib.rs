use ::proc_macro::TokenStream;
use ::proc_macro2::{TokenStream as TokenStream2};
use syn::{parse_macro_input, DeriveInput, Data, DataStruct, Field};
use quote::{quote, ToTokens};
use ::syn::{*, parse::{Parse, Parser}, spanned::Spanned, Result};

#[proc_macro_derive(HasStructure, attributes(seen_structure))]
pub fn has_structure_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as _);
    TokenStream::from(match impl_has_structure(ast) {
        | Ok(it) => it,
        | Err(err) => err.to_compile_error(),
    })
}

fn impl_has_structure(ast: DeriveInput) -> Result<TokenStream2> {
        let item_vis = ast.vis;
        let name = ast.ident;

        match ast.data {
            Data::Struct(s) => impl_has_structure_for_struct(item_vis, name, s),
            Data::Enum(e) => impl_has_structure_for_enum(item_vis, name, e),
            Data::Union(_) => todo!(),
        }
}

fn impl_has_structure_for_struct(item_vis: Visibility, name: Ident, s: DataStruct) -> Result<TokenStream2> {
    let structure_name = Ident::new(&format!("{}Structure", name), name.span());
    let structure_record_name = Ident::new(&format!("{}StructureRecord", name), name.span());

    let definition_fields: Vec<_> = s.fields.iter()
        .map(|field: &Field| {
            let ident = &field.ident;
            let ty = &field.ty;
            quote! { #ident: <#ty as HasStructure>::Structure }
        })
        .collect();

    let init_fields: Vec<_> = s.fields.iter()
        .map(|field: &Field| {
            let ident = &field.ident;
            quote! { #ident: self.#ident.structure() }
        })
        .collect();

    Ok({
        quote! {
            #[derive(Eq, PartialEq, ::std::hash::Hash)]
            #item_vis struct #structure_name {
                #(#definition_fields),*
            }

            impl ::seen_structure::ItemStructure for #structure_name {}

            #item_vis struct #structure_record_name {
                #(#definition_fields),*
            }

            impl ::seen_structure::HasStructure for #name {
                type Structure = #structure_name;

                fn structure(&self) -> Self::Structure {
                    Self::Structure {
                        #(#init_fields),*
                    }
                }
            }
        }
    })
}

fn impl_has_structure_for_enum(item_vis: Visibility, name: Ident, e: DataEnum) -> Result<TokenStream2> {
    let structure_name = Ident::new(&format!("{}Structure", name), name.span());
    let structure_record_name = Ident::new(&format!("{}StructureRecord", name), name.span());

    let monostate = Ident::new("Structure_Monostate", name.span());
    let mut monostate_added = false;
    let structure_variants: Vec<_> = e.variants.iter()
        .filter_map(|variant: &Variant| {
            // Unit variants don't have different structure. Don't generate a structure item for
            // them, but we will generate a Monostate variant if there are any unit variants.
            if variant.fields == Fields::Unit {
                if monostate_added {
                    None
                } else {
                    monostate_added = true;
                    Some(&monostate)
                }
            } else {
                Some(&variant.ident)
            }
        })
        .collect();

    let new_matches: Vec<_> = e.variants.iter()
        .map(|variant: &Variant| {
            let ident = &variant.ident;
            match &variant.fields {
                Fields::Named(_) => {
                    quote! { #name::#ident { .. } => #structure_name::#ident }
                }
                Fields::Unnamed(_) => {
                    quote! { #name::#ident(_) => #structure_name::#ident }
                }
                Fields::Unit => {
                    quote! { #name::#ident => #structure_name::#monostate }
                }
            }
        })
        .collect();

    Ok({
        quote! {
            #[derive(Eq, PartialEq, ::std::hash::Hash)]
            #item_vis enum #structure_name {
                #(#structure_variants,)*
            }

            impl ::seen_structure::ItemStructure for #structure_name {}

            #item_vis struct #structure_record_name {}

            impl ::seen_structure::HasStructure for #name {
                type Structure = #structure_name;

                fn structure(&self) -> Self::Structure {
                    match self {
                        #(#new_matches,)*
                    }
                }
            }
        }
    })
}