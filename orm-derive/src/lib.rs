#![forbid(unsafe_code)]
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput, Fields, LitStr};

#[proc_macro_derive(Object, attributes(table_name, column_name))]
pub fn derive_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    if let Data::Struct(ref data) = input.data {
        let type_name = &input.ident;
        let table_name = match parse_table_name(&input) {
            Ok(s) => s,
            Err(err) => return err.to_compile_error().into(),
        };

        let mut field_as_value = vec![];
        let mut field_from_value = vec![];
        let mut field_entries = vec![];
        for (i, field) in data.fields.iter().enumerate() {
            let column_name = match parse_column_name(field) {
                Ok(s) => s,
                Err(err) => return err.to_compile_error().into(),
            };

            let field_name = field
                .ident
                .as_ref()
                .map_or("unnamed_field".to_string(), |ident| ident.to_string());

            let field_type = &field.ty;
            let as_val = match &field.ident {
                Some(ident) => quote! {
                    <#field_type as orm::AsDataType>::as_value(&self.#ident),
                },
                None => quote! {
                    <#field_type as orm::AsDataType>::as_value(&self.#i),
                },
            };
            field_as_value.push(as_val);

            let from_val = match &field.ident {
                Some(ident) => quote! {
                    #ident: <#field_type as orm::AsDataType>::from_value(&row[#i]),
                },
                None => quote! {
                    <#field_type as orm::AsDataType>::from_value(&row[#i]),
                },
            };
            field_from_value.push(from_val);

            field_entries.push(quote! {
                orm::object::Field {
                    attr_name: #field_name,
                    column_name: #column_name,
                    column_type: <#field_type as orm::AsDataType>::DATA_TYPE,
                },

            });
        }

        let as_row = quote! {
            fn as_row(&self) -> orm::storage::Row {
                vec![#(#field_as_value)*]
            }
        };

        let from_row = match data.fields {
            Fields::Named(_) => {
                quote! {
                    fn from_row(row: orm::storage::Row) -> Self {
                        Self {#(#field_from_value)*}
                    }
                }
            }
            Fields::Unnamed(_) => {
                quote! {
                    fn from_row(row: orm::storage::Row) -> Self {
                        Self (#(#field_from_value)*)
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    fn from_row(row: orm::storage::Row) -> Self {
                        Self
                    }
                }
            }
        };

        let schema = quote! {
            const SCHEMA: orm::Schema = orm::Schema {
                type_name: stringify!(#type_name),
                table_name: #table_name,

                fields: &[#(#field_entries)* ],
            };
        };

        quote! {
            impl orm::Object for #type_name {
                #as_row

                #from_row

                #schema
            }
        }
        .into()
    } else {
        syn::Error::new(input.ident.span(), "Only structs can derive `Object`")
            .to_compile_error()
            .into()
    }
}

fn parse_table_name(input: &DeriveInput) -> syn::Result<String> {
    let type_name = &input.ident;
    let mut table_name = type_name.to_string();
    for attr in &input.attrs {
        match &attr.meta {
            syn::Meta::List(list) if attr.path().is_ident("table_name") => {
                table_name = match list.parse_args::<LitStr>() {
                    Ok(lit) => lit.value(),
                    Err(_) => {
                        return Err(syn::Error::new(
                            list.span(),
                            "Attribute argument should be a single string literal",
                        ));
                    }
                };
            }
            _ => {
                return Err(syn::Error::new(
                    attr.span(),
                    "Incorrect format for using `table_name` attribute. Usage: `#[table_name(\"MyTable\")]`"));
            }
        }
    }

    Ok(table_name)
}

fn parse_column_name(field: &syn::Field) -> syn::Result<String> {
    let mut column_name = field.ident.as_ref().map(|ident| ident.to_string());

    for attr in &field.attrs {
        match &attr.meta {
            syn::Meta::List(list) if attr.path().is_ident("column_name") => {
                column_name = match list.parse_args::<LitStr>() {
                    Ok(lit) => Some(lit.value()),
                    Err(_) => {
                        return Err(syn::Error::new(
                            list.span(),
                            "Attribute argument should be a single string literal",
                        ));
                    }
                };
            }
            _ => {
                return Err(syn::Error::new(
                    attr.span(),
                    "Incorrect format for using `column_name` attribute. Usage: `#[column_name(\"MyColumn\")]`"));
            }
        }
    }

    match column_name {
        Some(name) => Ok(name),
        None => Err(syn::Error::new(
            field.span(),
            "Fields of tuple structs should be marked with `column_name` attribute",
        )),
    }
}
