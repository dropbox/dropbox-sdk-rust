// Copyright (c) 2019-2025 Dropbox, Inc.

#![deny(missing_docs, rust_2018_idioms)]
// Enable a nightly feature for docs.rs which enables decorating feature-gated items.
// To enable this manually, run e.g. `cargo rustdoc --all-features -- --cfg docsrs`.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc = include_str!("../README.md"))]
#![cfg_attr(
    not(docsrs),
    doc = "Dropbox SDK for Rust. See README.md for more details."
)]

/// Feature-gate something and also decorate it with the feature name on docs.rs.
macro_rules! if_feature {
    ($feature_name:expr, $($item:item)*) => {
        $(
            #[cfg(feature = $feature_name)]
            #[cfg_attr(docsrs, doc(cfg(feature = $feature_name)))]
            $item
        )*
    };
    (not $feature_name:expr, $($item:item)*) => {
        $(
            #[cfg(not(feature = $feature_name))]
            #[cfg_attr(docsrs, doc(cfg(not(feature = $feature_name))))]
            $item
        )*
    };
}

#[macro_use]
extern crate log;

if_feature! { "default_client",
    pub mod default_client;

    // for backwards-compat only; don't match this for async
    if_feature! { "sync_routes_in_root",
        pub use client_trait::*;
    }
}

if_feature! { "default_async_client", pub mod default_async_client; }

#[cfg(any(feature = "default_client", feature = "default_async_client"))]
mod default_client_common;

mod client_trait_common;

pub mod client_trait;

pub mod async_client_trait;

mod client_helpers;

pub mod oauth2;

// You need to run the Stone generator to create this module.
mod generated;
pub use generated::*;

#[cfg(feature = "async_routes")]
#[cfg(not(feature = "sync_routes_in_root"))]
pub use async_routes::*;

#[cfg(feature = "sync_routes")]
#[cfg(feature = "sync_routes_in_root")]
pub use sync_routes::*;

mod error;
pub use error::{BoxedError, Error, NoError};
