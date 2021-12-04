// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse_macro_input,
    parse_quote,
    spanned::Spanned,
    Data,
    DataEnum,
    DataStruct,
    DeriveInput,
    Fields,
    GenericParam,
    Generics,
    Ident,
    Index,
    Variant,
};

mod internals;
use internals::{
    attr::{Adjacent, Rules, Tagged},
    case,
};

#[proc_macro_derive(ToCjson, attributes(cjson))]
pub fn cjson_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let rules = match Rules::from_input(&input) {
        Ok(rules) => rules,
        Err(err) => panic!("{}", err),
    };

    // Used in the quasi-quotation below as `#name`.
    let name = &input.ident;

    let generics = add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Generate an expression to sum up the heap size of each field.
    let cjson = cjson(&input.ident, &input.data, &rules);

    let expanded = quote! {
        // The generated impl.
        impl #impl_generics link_canonical::json::ToCjson for #name #ty_generics #where_clause {
            fn into_cjson(self) -> link_canonical::json::Value {
                #cjson
            }
        }
    };

    // Hand the output tokens back to the compiler.
    proc_macro::TokenStream::from(expanded)
}

// Add a bound `T: ToCjson` to every type parameter T.
fn add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param
                .bounds
                .push(parse_quote!(link_canonical::json::ToCjson));
        }
    }
    generics
}

fn cjson(ident: &Ident, data: &Data, rules: &Rules) -> TokenStream {
    match *data {
        Data::Struct(ref data) => cjson_struct(data, rules),
        Data::Enum(ref data) => cjson_enum(ident, data, rules),
        Data::Union(_) => unimplemented!(),
    }
}

/// Generate the `TokenStream` for a `struct` to form a
/// `link_canonical::json::Value`.
///
/// # Named Fields
///
/// If the `struct` has named fields, we first alias them using `let`
/// statements. For example, if we have `Foo { x: u64 }`, then a code block will
/// be generated that will look like:
///
/// ```rust,ignore
/// let x = self.x;
/// ```
///
/// All fields are collected to form a `Value::Object`.
///
/// ```rust,ignore
/// #[derive(ToCjson)]
/// struct Bar {
///   x: usize,
/// }
/// ```
///
/// ```json
/// { "x": 42 }
/// ```
///
/// # Unnamed Fields
///
/// Similar to named fields, we first alias the fields by their position. For
/// example, if we had `Foo(bool, u64)`, then the code block will look like:
///
/// ```rust,ignore
/// let __field0 = self.0;
/// let __field1 = self.1;
/// ```
///
/// All fields are collected to form a `Value::Array`. For example:
///
/// ```rust,ignore
/// #[derive(ToCjson)]
/// struct Bar(true, usize);
/// ```
///
/// ```json
/// [true, 42]
/// ```
///
/// # Unit Fields
///
/// These are simply output as `Value::Null`. For example:
///
/// ```rust,ignore
/// struct Bar;
/// ```
///
/// ```json
/// null
/// ```
fn cjson_struct(data: &DataStruct, rules: &Rules) -> TokenStream {
    match data.fields {
        Fields::Named(ref fields) => {
            let names = fields
                .named
                .iter()
                .cloned()
                .map(|field| field.ident.unwrap());
            let alias = names.clone().map(|name| {
                quote! { let #name = self.#name; }
            });
            let imp = product::named_fields(names, rules);
            quote! {
                #(#alias)*
                #imp
            }
        },
        Fields::Unnamed(ref fields) => {
            let names = fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, field)| Ident::new(&format!("__field{}", i), field.span()));
            let alias = names.clone().enumerate().map(|(i, name)| {
                let i = Index::from(i);
                quote! { let #name = self.#i; }
            });
            let imp = product::unnamed_fields(names.clone());
            quote! {
                #(#alias)*
                #imp
            }
        },
        Fields::Unit => {
            quote! { link_canonical::json::Value::Null }
        },
    }
}

/// Generate the `TokenStream` for a `enum` to form a
/// `link_canonical::json::Value`.
///
/// # Tagged Enums
///
/// All `enum`s must be either internally or adjacently tagged. The former means
/// that the variant of the enum is kept track of in an extra field, specified
/// using the `tag` attribute. The latter is similar to the internally tagged
/// method, however, the fields are embedded under a key specified by the
/// `content` attribute.
///
/// For example, the following `enum`s demonstrate the two methods:
///
/// ```rust,ignore
/// #[derive(ToCjson)]
/// #[cjson(tag = "type")]
/// enum Foo {
///   Bar { x: usize },
///   Baz { y: bool },
/// }
/// ```
///
/// ```json
/// { "type": "Bar", "x": 42 }
/// ```
///
/// ```json
/// { "type": "Baz", "y": true }
/// ```
///
/// ```rust,ignore
/// #[derive(ToCjson)]
/// #[cjson(tag = "type", content = "payload")]
/// enum Foo {
///   Bar(usize),
///   Baz(bool),
/// }
/// ```
///
/// ```json
/// { "type": "Bar", "payload": [42] }
/// ```
///
/// ```json
/// { "type": "Baz", "payload": [true] }
/// ```
///
/// **NOTE**: if any `enum` variant has unnamed fields, then it MUST use the
/// adjacently tagged method since the derive computation will not be able to
/// provide field names for the keys.
///
/// # Named Fields
///
/// If the `enum` has named fields, we match on their identifiers. If the `enum`
/// is internally tagged, then the tag and variant are added alongside the
/// fields, and are collected in a `Value::Object`. Otherwise, we embed the
/// fields as a `Value::Object` under the key specified by `content` and create
/// an outer `Value::Object` which includes the content and the `tag`.
///
/// # Unnamed Fields
///
/// Similar to named fields, we match on the variant, but we need to assign the
/// fields names. These are named `__field<n>` for each successive field.
/// All fields are collected to form a `Value::Array`. The `enum` must be
/// adjacently tagged, see above.
///
/// # Unit Fields
///
/// These are simply output with the `tag` as the key and the name of the
/// variant as the value. These can be either internally or adjacently tagged.
///
/// ```rust,ignore
/// #[derive(ToCjson)]
/// #[cjson(tag = "type")]
/// enum Foo {
///   Quux,
/// }
/// ```
///
/// ```json
/// { "type": "Quux" }
/// ```
#[rustfmt::skip::macros(quote)]
fn cjson_enum(ident: &Ident, data: &DataEnum, rules: &Rules) -> TokenStream {
    let tagged = match &rules.tagged {
        None => {
            panic!("expected #[cjson(tag = ...)] or #[cjson(tag = \"...\", content = \"...\")]")
        },
        Some(tagged) => tagged,
    };
    let arms = data
        .variants
        .iter()
        .map(|v| coproduct::variant(ident, tagged, rules.casing, v));

    quote! { match self { #(#arms),* } }
}

mod product {
    use super::*;

    #[rustfmt::skip::macros(quote)]
    pub fn named_fields(names: impl Iterator<Item = Ident>, rules: &Rules) -> TokenStream {
        let kvs = names.map(|name| {
            let cased = case::convert(&format!("{}", name), rules.casing);
            quote! { (#cased, link_canonical::json::ToCjson::into_cjson(#name)) }
        });
        quote! {
	    use std::iter::FromIterator as _;
	    let kvs = vec![#(#kvs),*].into_iter();
	    link_canonical::json::Value::from_iter(
		kvs.map(|(key, value)| {
		    (link_canonical::Cstring::from(key), value)
		})
	    )
	}
    }

    #[rustfmt::skip::macros(quote)]
    pub fn unnamed_fields(names: impl Iterator<Item = Ident>) -> TokenStream {
        let vs = names.map(|name| {
            quote! { link_canonical::json::ToCjson::into_cjson(#name) }
        });
        quote! {
	    use std::iter::FromIterator as _;
	    let mut val = link_canonical::json::Map::new();
	    let vs = vec![#(#vs),*].into_iter();
	    link_canonical::json::Value::Array(
		link_canonical::json::Array::from_iter(vs)
	    )
	}
    }
}

mod coproduct {
    use super::*;

    #[rustfmt::skip::macros(quote)]
    pub fn variant(
        ident: &Ident,
        tagged: &Tagged,
        casing: Option<case::Case>,
        variant: &Variant,
    ) -> TokenStream {
        let name = &variant.ident;
        match &variant.fields {
            Fields::Named(ref fields) => {
                let named = fields.named.iter().cloned().map(|f| f.ident.unwrap());
                let body = named_fields(variant, named.clone(), tagged, casing);
                quote! { #ident::#name { #(#named),* } => { #body } }
            },
            Fields::Unnamed(ref fields) => {
                let adj = match tagged {
                    Tagged::Internally(_) => {
                        panic!("expected #[cjson(tag = \"...\", content = \"...\")]")
                    },
                    Tagged::Adjacently(adj) => adj,
                };
                let named = (0..fields.unnamed.len())
                    .map(|i| Ident::new(&format!("__field{}", i), Span::call_site()));
                let body = unnamed_fields(variant, named.clone(), adj);
                quote! { #ident::#name ( #(#named),* ) => { #body } }
            },
            Fields::Unit => {
                let tag = tagged.tag();
                quote! {
		    #ident::#name => {
			let mut val = link_canonical::json::Map::new();
			val.insert(
			    link_canonical::Cstring::from(#tag),
			    link_canonical::json::ToCjson::into_cjson(stringify!(#name))
			);
			link_canonical::json::Value::Object(val)
		    }
		}
            },
        }
    }

    #[rustfmt::skip::macros(quote)]
    fn unnamed_fields(
        variant: &Variant,
        names: impl Iterator<Item = Ident>,
        Adjacent { tag, content }: &Adjacent,
    ) -> TokenStream {
        let vs = names.map(|name| {
            quote! { link_canonical::json::ToCjson::into_cjson(#name) }
        });
        let name = &variant.ident;
        quote! {
	    use std::iter::FromIterator as _;
	    let mut val = link_canonical::json::Map::new();
	    val.insert(
		link_canonical::Cstring::from(#tag),
		link_canonical::json::ToCjson::into_cjson(stringify!(#name))
	    );
	    let vs = vec![#(#vs),*].into_iter();
	    val.insert(
		link_canonical::Cstring::from(#content),
		link_canonical::json::Value::Array(
		    link_canonical::json::Array::from_iter(vs)
		)
	    );
	    link_canonical::json::Value::Object(val)
	}
    }

    #[rustfmt::skip::macros(quote)]
    fn named_fields(
        variant: &Variant,
        names: impl Iterator<Item = Ident>,
        tagged: &Tagged,
        casing: Option<case::Case>,
    ) -> TokenStream {
        let kvs = names.map(|name| {
            let cased = case::convert(&format!("{}", name), casing);
            quote! { (#cased, link_canonical::json::ToCjson::into_cjson(#name)) }
        });
        let name = &variant.ident;

        match tagged {
            Tagged::Internally(tag) => {
                let kvs = kvs.chain(std::iter::once(
                    quote! { (#tag, link_canonical::json::ToCjson::into_cjson(stringify!(#name))) },
                ));
                quote! {
		    use std::iter::FromIterator as _;
		    let kvs = vec![#(#kvs),*].into_iter();
		    link_canonical::json::Value::from_iter(
			kvs.map(|(key, value)| {
			    (link_canonical::Cstring::from(key), value)
			})
		    )
		}
            },
            Tagged::Adjacently(Adjacent { tag, content }) => {
                quote! {
		    use std::iter::FromIterator as _;
		    let mut val = link_canonical::json::Map::new();
		    val.insert(
			link_canonical::Cstring::from(#tag),
			link_canonical::json::ToCjson::into_cjson(stringify!(#name))
		    );
		    let kvs = vec![#(#kvs),*].into_iter();
		    val.insert(
			link_canonical::Cstring::from(#content),
			link_canonical::json::Value::from_iter(
			    kvs.map(|(key, value)| {
				(link_canonical::Cstring::from(key), value)
			    })
			)
		    );
		    link_canonical::json::Value::Object(val)
		}
            },
        }
    }
}
