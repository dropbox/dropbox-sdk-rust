// Copyright (c) 2019-2021 Dropbox, Inc.

#![deny(
    missing_docs,
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

#[macro_use] extern crate log;

/// An error occurred in the process of making an API call.
/// This is different from the case where your call succeeded, but the operation returned an error.
#[derive(thiserror::Error, Debug)]
pub enum Error {

    /// Some error from the internals of the HTTP client.
    #[error("error from HTTP client: {0}")]
    HttpClient(Box<dyn std::error::Error + Send + Sync + 'static>),

    /// Something went wrong in the process of transforming your arguments into a JSON string.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// The Dropbox API response was unexpected or malformed in some way.
    #[error("Dropbox API returned something unexpected: {0}")]
    UnexpectedResponse(&'static str),

    /// The Dropbox API indicated that your request was malformed in some way.
    #[error("Dropbox API indicated that the request was malformed: {0}")]
    BadRequest(String),

    /// Your access token is invalid.
    #[error("Dropbox API indicated that the access token is bad: {0}")]
    InvalidToken(String),

    /// Your request was rejected due to rate-limiting. You can retry it later.
    #[error("Dropbox API declined the request due to rate-limiting ({reason}), \
        retry after {retry_after_seconds}s")]
    RateLimited {
        /// The server-given reason for the rate-limiting.
        reason: String,

        /// You can retry this request after this many seconds.
        retry_after_seconds: u32,
    },

    /// The user or team account doesn't have access to the endpoint or feature.
    /// Value is a JSON object which matches the [`auth::AccessError`] type (if that namespace
    /// feature is enabled).
    #[error("Dropbox API denied access to the resource: {0}")]
    AccessDenied(serde_json::Value),

    /// The Dropbox API server had an internal error.
    #[error("Dropbox API had an internal server error: {0}")]
    ServerError(String),

    /// The Dropbox API returned an unexpected HTTP response code.
    #[error("Dropbox API returned HTTP {code} {status} - {json}")]
    UnexpectedHttpError {
        /// HTTP status code returned.
        code: u16,

        /// The HTTP status string.
        status: String,

        /// The response body.
        json: String,
    },
}

/// Shorthand for a Result where the error type is this crate's [`Error`] type.
pub type Result<T> = std::result::Result<T, Error>;

if_feature! { "default_client", pub mod default_client; }

pub mod client_trait;
pub use client_trait::{AppAuthClient, NoauthClient, UserAuthClient, TeamAuthClient};
pub(crate) mod client_helpers;
pub mod oauth2;

mod generated; // You need to run the Stone generator to create this module.
pub use generated::*;

/// A special error type for a method that doesn't have any defined error return. You can't
/// actually encounter a value of this type in real life; it's here to satisfy type requirements.
#[derive(Copy)]
pub enum NoError {}

impl Clone for NoError {
    fn clone(&self) -> NoError {
        unreachable(*self)
    }
}

impl std::cmp::PartialEq<NoError> for NoError {
    fn eq(&self, _: &NoError) -> bool {
        unreachable(*self)
    }
}

impl std::error::Error for NoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        unreachable(*self)
    }

    fn description(&self) -> &str {
        unreachable(*self)
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        unreachable(*self)
    }
}

impl std::fmt::Debug for NoError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unreachable(*self)
    }
}

impl std::fmt::Display for NoError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unreachable(*self)
    }
}

// This is the reason we can't just use the otherwise-identical `void` crate's Void type: we need
// to implement this trait.
impl<'de> serde::de::Deserialize<'de> for NoError {
    fn deserialize<D: serde::de::Deserializer<'de>>(_: D)
        -> std::result::Result<Self, D::Error>
    {
        Err(serde::de::Error::custom(
                "method has no defined error type, but an error was returned"))
    }
}

#[inline(always)]
fn unreachable(x: NoError) -> ! {
    match x {}
}
