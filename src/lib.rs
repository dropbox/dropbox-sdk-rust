// Copyright (c) 2019-2021 Dropbox, Inc.

#![deny(
    missing_docs,
    rust_2018_idioms,
)]

// Enable a nightly feature for docs.rs which enables decorating feature-gated items.
// To enable this manually, run e.g. `cargo rustdoc --all-features -- --cfg docsrs`.
#![cfg_attr(docsrs, feature(doc_cfg))]

// As of Rust 1.56, we can do #![doc = include_str!("../README.md")] to include README.md verbatim.
// But this is too new of a MSRV, so we're still gating it on the docsrs flag for now. Note the
// double cfg_attr gate, which is needed because feature(extended_key_value_attributes) makes a
// change to how this syntax is parsed in older compilers.
#![cfg_attr(docsrs, feature(extended_key_value_attributes))]
#![cfg_attr(docsrs, cfg_attr(docsrs, doc = include_str!("../README.md")))]
#![cfg_attr(not(docsrs), doc = "Dropbox SDK for Rust. See README.md for more details.")]

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

#[macro_use] extern crate log;

/// An error occurred in the process of making an API call.
/// This is different from the case where your call succeeded, but the operation returned an error.
#[derive(thiserror::Error, Debug)]
pub enum Error<E = NoError> {
    /// An error returned by the API. Its type depends on the endpoint being called.
    #[error("Dropbox API endpoint returned an error: {0}")]
    Api(E),

    /// Some error from the internals of the HTTP client.
    #[error("error from HTTP client: {0}")]
    HttpClient(Box<dyn std::error::Error + Send + Sync + 'static>),

    /// Something went wrong in the process of transforming your arguments into a JSON string.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// The Dropbox API response was unexpected or malformed in some way.
    #[error("Dropbox API returned something unexpected: {0}")]
    UnexpectedResponse(String),

    /// The Dropbox API indicated that your request was malformed in some way.
    #[error("Dropbox API indicated that the request was malformed: {0}")]
    BadRequest(String),

    /// Errors occurred during authentication.
    #[error("Dropbox API indicated a problem with authentication: {0}")]
    Authentication(types::auth::AuthError),

    /// Your request was rejected due to rate-limiting. You can retry it later.
    #[error("Dropbox API declined the request due to rate-limiting ({reason}), \
        retry after {retry_after_seconds}s")]
    RateLimited {
        /// The server-given reason for the rate-limiting.
        reason: types::auth::RateLimitReason,

        /// You can retry this request after this many seconds.
        retry_after_seconds: u32,
    },

    /// The user or team account doesn't have access to the endpoint or feature.
    #[error("Dropbox API denied access to the resource: {0}")]
    AccessDenied(types::auth::AccessError),

    /// The Dropbox API server had an internal error.
    #[error("Dropbox API had an internal server error: {0}")]
    ServerError(String),

    /// The Dropbox API returned an unexpected HTTP response code.
    #[error("Dropbox API returned HTTP {code} - {response}")]
    UnexpectedHttpError {
        /// HTTP status code returned.
        code: u16,

        /// The response body.
        response: String,
    },
}

impl<E: std::error::Error + 'static> Error<E> {
    /// Look for an inner error of the given type anywhere within this error, by walking the chain
    /// of [`Error::source`] recursively until something matches the desired type.
    pub fn downcast_ref_inner<E2: std::error::Error + 'static>(&self) -> Option<&E2> {
        let mut inner = Some(self as &dyn std::error::Error);
        while let Some(e) = inner {
            if let Some(e) = e.downcast_ref() {
                return Some(e);
            }
            inner = e.source();
        }
        None
    }

    /// Change the concretely-typed API error, if any, into a boxed trait object.
    ///
    /// This makes it possible to combine dissimilar errors into one type, which can be broken out
    /// later using [`Error::downcast_ref`] if desired.
    pub fn boxed(self) -> Error<Box<dyn std::error::Error>> {
        match self {
            Error::Api(e) => Error::Api(Box::new(e)),

            // Other variants unchanged.
            // These have to be actually re-stated, because the (unstated) generic type of `Error`
            // is different on the left vs the right.
            Error::HttpClient(e) => Error::HttpClient(e),
            Error::Json(e) => Error::Json(e),
            Error::UnexpectedResponse(e) => Error::UnexpectedResponse(e),
            Error::BadRequest(e) => Error::BadRequest(e),
            Error::Authentication(e) => Error::Authentication(e),
            Error::RateLimited { reason, retry_after_seconds } => Error::RateLimited { reason, retry_after_seconds },
            Error::AccessDenied(e) => Error::AccessDenied(e),
            Error::ServerError(e) => Error::ServerError(e),
            Error::UnexpectedHttpError { code, response } => Error::UnexpectedHttpError { code, response },
        }
    }
}

impl Error<NoError> {
    /// Lift an error with no possible API error value to a typed error of any type.
    ///
    /// Ideally this would just be `impl<E> From<Error<NoError>> for Error<E>` but that conflicts
    /// with the reflexive conversion (E could be NoError), and Rust doesn't have negative type
    /// bounds or specialization, so it has to be this method instead.
    pub fn typed<E>(self) -> Error<E> {
        match self {
            Error::Api(x) => unreachable(x),
            Error::HttpClient(e) => Error::HttpClient(e),
            Error::Json(e) => Error::Json(e),
            Error::UnexpectedResponse(e) => Error::UnexpectedResponse(e),
            Error::BadRequest(e) => Error::BadRequest(e),
            Error::Authentication(e) => Error::Authentication(e),
            Error::RateLimited { reason, retry_after_seconds } => Error::RateLimited { reason, retry_after_seconds },
            Error::AccessDenied(e) => Error::AccessDenied(e),
            Error::ServerError(e) => Error::ServerError(e),
            Error::UnexpectedHttpError { code, response } => Error::UnexpectedHttpError { code, response },
        }
    }
}

if_feature! { "default_client",
    pub mod default_client;

    // for backwards-compat only; don't match this for async
    if_feature! { "sync_routes_default",
        pub use client_trait::*;
    }
}

if_feature! { "default_async_client", pub mod default_async_client; }

#[cfg(any(feature = "default_client", feature = "default_async_client"))]
pub(crate) mod default_client_common;

pub mod client_trait_common;

pub mod client_trait;

pub mod async_client_trait;

pub(crate) mod client_helpers;
pub mod oauth2;

mod generated;

// You need to run the Stone generator to create this module.
pub use generated::*;

/// A special error type for a method that doesn't have any defined error return. You can't
/// actually encounter a value of this type in real life; it's here to satisfy type requirements.
#[derive(Copy, Clone)]
pub enum NoError {}

impl PartialEq<NoError> for NoError {
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
