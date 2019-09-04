// Copyright (c) 2019 Dropbox, Inc.

#![deny(rust_2018_idioms)]

#[macro_use] extern crate error_chain;
#[macro_use] extern crate log;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    foreign_links {
        Hyper(hyper::Error) #[cfg(feature = "hyper_client")]; // TODO: this should be made more abstract
        Io(std::io::Error);
        Json(serde_json::Error);
        Utf8(std::string::FromUtf8Error);
    }

    errors {
        /// The API returned something invalid.
        UnexpectedError(reason: &'static str) {
            description("Dropbox unexpected API error")
            display("Dropbox unexpected API error: {}", reason)
        }

        /// The API indicated that the request was malformed.
        BadRequest(message: String) {
            description("Dropbox returned 400 Bad Request")
            display("Dropbox returned 400 Bad Request: {}", message)
        }

        /// The API indicated that the access token is bad.
        InvalidToken(message: String) {
            description("Dropbox API token is invalid, expired, or revoked")
            display("Dropbox API token is invalid, expired, or revoked: {}", message)
        }

        /// The API declined the request due to rate-limiting.
        RateLimited(reason: String) {
            description("Dropbox denied the request due to rate-limiting")
            display("Dropbox denied the request due to rate-limiting: {}", reason)
        }

        /// The API had an internal server error.
        ServerError(message: String) {
            description("Dropbox had an internal server error")
            display("Dropbox had an internal server error: {}", message)
        }

        /// The API returned an unexpected HTTP error code.
        GeneralHttpError(code: u16, status: String, json: String) {
            description("Dropbox API returned failure")
            display("Dropbox API returned HTTP {} {} - {}", code, status, json)
        }
    }
}

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
