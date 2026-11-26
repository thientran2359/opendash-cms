use super::*;
use crate::{
    builder::{BindingSource, ListSort, ResponsiveStyle, TextAlign},
    registry::ResourceOwner,
};
use serde_json::json;

fn el(id: &str, content: KnownElement) -> ElementNode {
    ElementNode {
        id: id.into(),
        meta: None,
        content: ElementContent::Known(content),
    }
}

mod suite_01;
mod suite_02;
