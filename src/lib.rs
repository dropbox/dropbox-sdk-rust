// Copyright (c) 2019-2020 Dropbox, Inc.

#![deny(
    broken_intra_doc_links,
    rust_2018_idioms,
)]

// Enable a nightly-only feature for docs.rs which enables inlining an external file into
// documentation.
#![cfg_attr(docsrs, feature(external_doc))]

// Then if that is available, inline the entirety of README.md; otherwise, include a short blurb
// that simply references it.
#![cfg_attr(docsrs, doc(include = "../README.md"))]
#![cfg_attr(not(docsrs), doc = "Dropbox SDK for Rust. See README.md for more details.")]

// Enable a nightly feature for docs.rs which enables decorating feature-gated items.
// To enable this manually, run e.g. `cargo rustdoc --all-features -- --cfg docsrs`.
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Feature-gate something and also decorate it with the feature name on docs.rs.
macro_rules! if_feature {
    ($feature_name:expr, $($item:item)*) => {
        $(
            #[cfg(feature = $feature_name)]
            #[cfg_attr(docsrs, doc(cfg(feature = $feature_name)))]
            $item
        )*
    }
}

use thiserror::Error;
#[macro_use] extern crate log;

#[derive(Error, Debug)]
pub enum Error {

    #[error("error from HTTP client: {0}")]
    HttpClient(Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Dropbox API returned something unexpected: {0}")]
    UnexpectedResponse(&'static str),

    #[error("Dropbox API indicated that the request was malformed: {0}")]
    BadRequest(String),

    #[error("Dropbox API indicated that the access token is bad: {0}")]
    InvalidToken(String),

    #[error("Dropbox API declined the request due to rate-limiting ({reason}), \
        retry after {retry_after_seconds}s")]
    RateLimited { reason: String, retry_after_seconds: u32 },

    #[error("Dropbox API had an internal server error: {0}")]
    ServerError(String),

    #[error("Dropbox API returned HTTP {code} {status} - {json}")]
    UnexpectedHttpError {
        code: u16,
        status: String,
        json: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

if_feature! { "default_client", pub mod default_client; }

pub mod client_trait;
pub use client_trait::{AppAuthClient, NoauthClient, UserAuthClient, TeamAuthClient};
pub(crate) mod client_helpers;
pub mod oauth2;

mod generated; // You need to run the Stone generator to create this module.
pub use generated::*;
