// Copyright (c) 2019-2020 Dropbox, Inc.

//! Everything needed to implement your HTTP client.

use std::io::Read;
use std::sync::Arc;
pub use crate::client_trait_common::{HttpRequest, TeamSelect};
use crate::Error;

/// The base HTTP synchronous client trait.
pub trait HttpClient: Sync {
    /// The concrete type of request supported by the client.
    type Request: HttpRequest + Send;

    /// Make a HTTP request.
    fn execute(
        &self,
        request: Self::Request,
        body: &[u8],
    ) -> Result<HttpRequestResultRaw, Error>;

    /// Create a new request instance for the given URL. It should be a POST request.
    fn new_request(&self, url: &str) -> Self::Request;

    /// Attempt to update the current authentication token. The previously fetched token is given
    /// as a way to avoid repeat updates in case of a race. If the update is successful, return
    /// `true` and the current request will be retried with a newly-fetched token. Return `false` if
    /// authentication is not supported, or return an error if the update operation fails.
    fn update_token(&self, _old_token: Arc<String>) -> Result<bool, Error> {
        Ok(false)
    }

    /// The client's current authentication token, if any.
    fn token(&self) -> Option<Arc<String>> {
        None
    }

    /// The currently set path root, if any.
    fn path_root(&self) -> Option<&str> {
        None
    }

    /// The alternate user or team context currently set, if any.
    fn team_select(&self) -> Option<&TeamSelect> {
        None
    }
}

/// Marker trait to indicate that a HTTP client supports unauthenticated routes.
pub trait NoauthClient: HttpClient {}

/// Marker trait to indicate that a HTTP client supports User authentication.
/// Team authentication works by adding a `Authorization: Bearer <TOKEN>` header.
pub trait UserAuthClient: HttpClient {}

/// Marker trait to indicate that a HTTP client supports Team authentication.
/// Team authentication works by adding a `Authorization: Bearer <TOKEN>` header, and optionally a
/// `Dropbox-API-Select-Admin` or `Dropbox-API-Select-User` header.
pub trait TeamAuthClient: HttpClient {}

/// Marker trait to indicate that a HTTP client supports App authentication.
/// App authentication works by adding a `Authorization: Basic <base64(APP_KEY:APP_SECRET)>` header
/// to the HTTP request.
pub trait AppAuthClient: HttpClient {}

/// The raw response from the server, including a sync streaming response body.
pub struct HttpRequestResultRaw {
    /// HTTP response code.
    pub status: u16,

    /// The value of the `Dropbox-API-Result` header, if present.
    pub result_header: Option<String>,

    /// The value of the `Content-Length` header in the response, if present.
    pub content_length: Option<u64>,

    /// The response body stream.
    pub body: Box<dyn Read + Send>,
}

/// The response from the server, parsed into a given type, including a body stream if it is from
/// a Download style request.
pub struct HttpRequestResult<T> {
    /// The API result, parsed into the given type.
    pub result: T,

    /// The value of the `Content-Length` header in the response, if any. Only expected to not be
    /// `None` if `body` is also not `None`.
    pub content_length: Option<u64>,

    /// The response body stream, if any. Only expected to not be `None` for
    /// [`Style::Download`](crate::client_trait_common::Style::Download) endpoints.
    pub body: Option<Box<dyn Read>>,
}

