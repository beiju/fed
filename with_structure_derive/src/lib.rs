use ::proc_macro::TokenStream;
use ::proc_macro2::TokenStream as TokenStream2;
use ::syn::{Result, *};
use darling::FromField;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DataStruct, DeriveInput, Field, parse_macro_input};

#[proc_macro_derive(WithStructure, attributes(with_structure))]
pub fn with_structure_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as _);
    TokenStream::from(impl_with_structure(ast).unwrap_or_else(|err| err.to_compile_error()))
}

fn impl_with_structure(ast: DeriveInput) -> Result<TokenStream2> {
    let item_vis = ast.vis;
    let name = ast.ident;
    let generics = ast.generics;

    match ast.data {
        Data::Struct(s) => impl_with_structure_for_struct(item_vis, name, generics, s),
        // TODO Enum generics too
        Data::Enum(e) => impl_with_structure_for_enum(item_vis, name, generics, e),
        Data::Union(_) => todo!(),
    }
}

#[derive(Debug, FromField)]
#[darling(attributes(with_structure))]
struct WithStructureOpts {
    ignore: darling::util::Flag,
}

fn definition_field_from_item_field(field: &Field) -> Option<TokenStream2> {
    let opts = WithStructureOpts::from_field(field).unwrap();
    if opts.ignore.is_present() {
        return None;
    }

    let ident_opt = &field.ident;
    let ty = &field.ty;
    Some(if let Some(ident) = ident_opt {
        quote! { #ident: <#ty as WithStructure>::Structure }
    } else {
        quote! { <#ty as WithStructure>::Structure }
    })
}

fn impl_with_structure_for_struct(
    item_vis: Visibility,
    name: Ident,
    generics: Generics,
    s: DataStruct,
) -> Result<TokenStream2> {
    let structure_name = Ident::new(&format!("{}Structure", name), name.span());

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let definition_fields: Vec<_> = s
        .fields
        .iter()
        .flat_map(definition_field_from_item_field)
        .collect();

    let init_fields: Vec<_> = s
        .fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: self.#ident.structure() }
        })
        .collect();

    Ok({
        quote! {
            #[::with_structure::perfect_derive::perfect_derive(Eq, PartialEq, Hash, Debug)]
            #[derive(::serde::Serialize)]
            #item_vis struct #structure_name #generics #where_clause {
                #(#definition_fields),*
            }

            impl #impl_generics ::with_structure::WithStructure for #name #ty_generics #where_clause {
                type Structure = #structure_name #ty_generics;

                fn structure(&self) -> Self::Structure {
                    Self::Structure {
                        #(#init_fields),*
                    }
                }
            }
        }
    })
}

fn impl_with_structure_for_enum(
    item_vis: Visibility,
    name: Ident,
    generics: Generics,
    e: DataEnum,
) -> Result<TokenStream2> {
    let structure_name = Ident::new(&format!("{}Structure", name), name.span());

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let structure_variants: Vec<_> = e
        .variants
        .iter()
        .map(|variant: &Variant| {
            let variant_ident = &variant.ident;
            match &variant.fields {
                Fields::Named(fields) => {
                    let structure_fields: Vec<_> = fields
                        .named
                        .iter()
                        .flat_map(definition_field_from_item_field)
                        .collect();

                    quote! {
                        #variant_ident {
                            #(#structure_fields),*
                        }
                    }
                }
                Fields::Unnamed(fields) => {
                    let structure_fields: Vec<_> = fields
                        .unnamed
                        .iter()
                        .map(definition_field_from_item_field)
                        .collect();

                    quote! {
                        #variant_ident(#(#structure_fields),*)
                    }
                }
                Fields::Unit => {
                    quote! {
                        #variant_ident
                    }
                }
            }
        })
        .collect();

    let init_match_branches: Vec<_> = e.variants.iter()
        .map(|variant: &Variant| {
            let ident = &variant.ident;
            match &variant.fields {
                Fields::Named(fields) => {
                    let destructuring_names: Vec<_> = fields.named.iter()
                        .map(|field| {
                            let opts = WithStructureOpts::from_field(field).unwrap();
                            let ident = field.ident.as_ref()
                                .expect("Fields in a named-field struct must be named");

                            if opts.ignore.is_present() {
                                quote! { #ident: _ }
                            } else {
                                quote! { #ident }
                            }
                        })
                        .collect();
                    let field_initializers: Vec<_> = fields.named.iter()
                        .flat_map(|field| {
                            let opts = WithStructureOpts::from_field(field).unwrap();
                            if opts.ignore.is_present() { return None }

                            let ident = &field.ident;
                            // Here the first #ident refers to the <UserEnum>Structure field and the second
                            // refers to the <UserEnum> field from the match destructuring
                            Some(quote! { #ident: #ident.structure() })
                        })
                        .collect();
                    quote! {
                        #name::#ident { #(#destructuring_names),* } => #structure_name::#ident {
                            #(#field_initializers),*
                        }
                    }
                }
                Fields::Unnamed(fields) => {
                    let destructuring_names: Vec<_> = fields.unnamed.iter()
                        .enumerate()
                        .flat_map(|(i, field)| {
                            let opts = WithStructureOpts::from_field(field).unwrap();
                            if opts.ignore.is_present() { return None }

                            Some(Ident::new(&format!("_{i}"), field.ty.span()))
                        })
                        .collect();
                    let field_initializers: Vec<_> = destructuring_names.iter()
                        .map(|ident| {
                            quote! { #ident.structure() }
                        })
                        .collect();
                    quote! {
                        #name::#ident (#(#destructuring_names),*) => #structure_name::#ident(#(#field_initializers),*)
                    }
                }
                Fields::Unit => {
                    quote! { #name::#ident => #structure_name::#ident }
                }
            }
        })
        .collect();

    Ok({
        quote! {
            #[::with_structure::perfect_derive::perfect_derive(Eq, PartialEq, Hash, Debug)]
            #[derive(::serde::Serialize)]
            #[allow(non_camel_case_types)]
            #item_vis enum #structure_name #generics #where_clause {
                #(#structure_variants,)*
            }

            impl #impl_generics ::with_structure::WithStructure for #name #ty_generics #where_clause {
                type Structure = #structure_name #ty_generics;

                fn structure(&self) -> Self::Structure {
                    match self {
                        #(#init_match_branches,)*
                    }
                }
            }
        }
    })
}
