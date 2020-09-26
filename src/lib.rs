// Copyright (c) 2019-2020 Dropbox, Inc.

#![deny(rust_2018_idioms)]

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

#[cfg(feature = "hyper_client")] pub mod hyper_client;

pub mod client_trait;
pub use client_trait::{AppAuthClient, NoauthClient, UserAuthClient, TeamAuthClient};
pub(crate) mod client_helpers;

mod generated; // You need to run the Stone generator to create this module.
pub use generated::*;
