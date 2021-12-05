// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use syn::{Attribute, DeriveInput, Lit, Meta, MetaNameValue, NestedMeta};

use crate::internals::case::Case;

pub const CJSON: &str = "cjson";
pub const RENAME_ALL: &str = "rename_all";
pub const TAGGED: &str = "tag";
pub const CONTENT: &str = "content";

/// The rules given by `cjson` attributes.
#[derive(Clone, Debug)]
pub struct Rules {
    /// Determined by the `rename_all` attribute.
    pub casing: Option<Case>,
    /// Determined by the `tag` and `content` attributes.
    pub tagged: Option<Tagged>,
}

/// The tagging style for an `enum`. `tag` is the minimal requirement, where
/// `content` is optional for named field variants and mandatory for unnamed
/// field variants.
#[derive(Clone, Debug)]
pub enum Tagged {
    /// If the attribute specified is `#[cjson(tag = ...)]` then the `enum` is
    /// internally tagged.
    Internally(String),
    /// If the attributes specified are `#[cjson(tag = ..., content = ...)]`
    /// then the `enum` is adjacently tagged.
    Adjacently(Adjacent),
}

#[derive(Clone, Debug)]
pub struct Adjacent {
    pub tag: String,
    pub content: String,
}

impl Tagged {
    fn new(tagged: Option<String>, content: Option<String>) -> Option<Self> {
        match (tagged, content) {
            (None, None) => None,
            (Some(tag), None) => Some(Self::Internally(tag)),
            (Some(tag), Some(content)) => Some(Self::Adjacently(Adjacent { tag, content })),
            _ => None,
        }
    }

    pub fn tag(&self) -> &String {
        match self {
            Self::Internally(tag) => tag,
            Self::Adjacently(Adjacent { tag, .. }) => tag,
        }
    }
}

impl Rules {
    pub fn new() -> Self {
        Rules {
            casing: None,
            tagged: None,
        }
    }

    pub fn from_input(input: &DeriveInput) -> Result<Self, &'static str> {
        let mut rules = Rules::new();
        let metas = input.attrs.iter().flat_map(get_meta_items);
        let mut tag = None;
        let mut content = None;

        for meta in metas {
            match meta {
                NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident(RENAME_ALL) => {
                    let casing = rename_all_rule(&m)?;
                    rules.casing = Some(casing);
                },
                NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident(TAGGED) => {
                    if let Lit::Str(t) = &m.lit {
                        tag = Some(t.value());
                    }
                },
                NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident(CONTENT) => {
                    if let Lit::Str(c) = &m.lit {
                        content = Some(c.value());
                    }
                },
                _ => {},
            }
        }

        rules.tagged = Tagged::new(tag, content);
        Ok(rules)
    }
}

pub fn get_meta_items(attr: &Attribute) -> Vec<NestedMeta> {
    if !attr.path.is_ident(CJSON) {
        return Vec::new();
    }

    match attr.parse_meta() {
        Ok(Meta::List(meta)) => meta.nested.into_iter().collect(),
        Ok(_) => {
            panic!("expected #[cjson(...)]")
        },
        Err(err) => {
            panic!("{}", err)
        },
    }
}

pub fn rename_all_rule(meta: &MetaNameValue) -> Result<Case, &'static str> {
    if let Lit::Str(casing) = &meta.lit {
        casing.value().parse()
    } else {
        Err("expected #[cjson(rename_all = <string>)], but <string> was not the correct type")
    }
}
