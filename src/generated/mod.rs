// DO NOT EDIT
// This file was @generated by Stone

#![allow(
    clippy::too_many_arguments,
    clippy::large_enum_variant,
    clippy::result_large_err,
    clippy::doc_markdown,
)]

pub mod types;

if_feature! { "async_routes",
    pub mod async_routes;
}

if_feature! { "sync_routes",
    pub mod sync_routes;
}

pub(crate) fn eat_json_fields<'de, V>(map: &mut V) -> Result<(), V::Error> where V: ::serde::de::MapAccess<'de> {
    while map.next_entry::<&str, ::serde_json::Value>()?.is_some() {
        /* ignore */
    }
    Ok(())
}
