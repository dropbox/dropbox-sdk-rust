// Copyright (c) 2019 Dropbox, Inc.

#![deny(rust_2018_idioms)]

use thiserror::Error;
#[macro_use] extern crate log;

#[derive(Error, Debug)]
pub enum Error {

    #[cfg(feature = "hyper_client")]
    #[error("error from HTTP client: {0}")]
    Hyper(#[from] hyper::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid UTF-8 string")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Dropbox API returned something unexpected: {0}")]
    UnexpectedResponse(&'static str),

    #[error("Dropbox API indicated that the request was malformed: {0}")]
    BadRequest(String),

    #[error("Dropbox API indicated that the access token is bad: {0}")]
    InvalidToken(String),

    #[error("Dropbox API declined the request due to rate-limiting: {0}")]
    RateLimited(String),

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

#[cfg(feature = "hyper_client")] mod hyper_client;
#[cfg(feature = "hyper_client")] pub use hyper_client::{
    HyperClient,
    Oauth2AuthorizeUrlBuilder,
    Oauth2Type,
};

pub mod client_trait;
pub(crate) mod client_helpers;

mod generated; // You need to run the Stone generator to create this module.
pub use generated::*;
