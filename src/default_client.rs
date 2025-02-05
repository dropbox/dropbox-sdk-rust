// Copyright (c) 2020-2025 Dropbox, Inc.

//! The default HTTP client.
//!
//! Use this client if you're not particularly picky about implementation details, as the specific
//! implementation is not exposed, and may be changed in the future.
//!
//! If you have a need for a specific HTTP client implementation, or your program is already using
//! some HTTP client crate, you probably want to have this Dropbox SDK crate use it as well. To do
//! that, you should implement the traits in `crate::client_trait` for it and use it instead.
//!
//! This code (and its dependencies) are only built if you use the `default_client` Cargo feature.

use crate::client_trait::{
    AppAuthClient, HttpClient, HttpRequest, HttpRequestResultRaw, NoauthClient, TeamAuthClient,
    TeamSelect, UserAuthClient,
};
use crate::default_client_common::impl_set_path_root;
use crate::oauth2::{Authorization, TokenCache};
use crate::Error;
use futures::FutureExt;
use std::str::FromStr;
use std::sync::Arc;
use ureq::typestate::WithBody;
use ureq::Agent;

macro_rules! impl_update_token {
    ($self:ident) => {
        fn update_token(&$self, old_token: Arc<String>) -> Result<bool, Error> {
            info!("refreshing auth token");
            match $self.tokens.update_token(
                TokenUpdateClient { inner: &$self.inner },
                old_token,
            ).now_or_never().unwrap() {
                Ok(_) => Ok(true),
                Err(e) => {
                    error!("failed to update auth token: {e}");
                    Err(e.into())
                }
            }
        }
    };
}

/// Default HTTP client using User authorization.
pub struct UserAuthDefaultClient {
    inner: UreqClient,
    tokens: Arc<TokenCache>,
    path_root: Option<String>, // a serialized PathRoot enum
}

impl UserAuthDefaultClient {
    /// Create a new client using the given OAuth2 authorization.
    pub fn new(auth: Authorization) -> Self {
        Self::from_token_cache(Arc::new(TokenCache::new(auth)))
    }

    /// Create a new client from a [`TokenCache`], which lets you share the same tokens between
    /// multiple clients.
    pub fn from_token_cache(tokens: Arc<TokenCache>) -> Self {
        Self {
            inner: UreqClient::default(),
            tokens,
            path_root: None,
        }
    }

    impl_set_path_root!(self);
}

impl HttpClient for UserAuthDefaultClient {
    type Request = UreqRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        self.inner.execute(request, body)
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.inner.new_request(url)
    }

    impl_update_token!(self);

    fn token(&self) -> Option<Arc<String>> {
        self.tokens.get_token()
    }

    fn path_root(&self) -> Option<&str> {
        self.path_root.as_deref()
    }
}

impl UserAuthClient for UserAuthDefaultClient {}

/// Default HTTP client using Team authorization.
pub struct TeamAuthDefaultClient {
    inner: UreqClient,
    tokens: Arc<TokenCache>,
    path_root: Option<String>, // a serialized PathRoot enum
    team_select: Option<TeamSelect>,
}

impl TeamAuthDefaultClient {
    /// Create a new client using the given OAuth2 token, with no user/admin context selected.
    pub fn new(tokens: impl Into<Arc<TokenCache>>) -> Self {
        Self {
            inner: UreqClient::default(),
            tokens: tokens.into(),
            path_root: None,
            team_select: None,
        }
    }

    /// Select a user or team context to operate in.
    pub fn select(&mut self, team_select: Option<TeamSelect>) {
        self.team_select = team_select;
    }

    impl_set_path_root!(self);
}

impl HttpClient for TeamAuthDefaultClient {
    type Request = UreqRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        self.inner.execute(request, body)
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.inner.new_request(url)
    }

    fn token(&self) -> Option<Arc<String>> {
        self.tokens.get_token()
    }

    impl_update_token!(self);

    fn path_root(&self) -> Option<&str> {
        self.path_root.as_deref()
    }

    fn team_select(&self) -> Option<&TeamSelect> {
        self.team_select.as_ref()
    }
}

impl TeamAuthClient for TeamAuthDefaultClient {}

/// Default HTTP client using App authorization.
#[derive(Debug)]
pub struct AppAuthDefaultClient {
    inner: UreqClient,
    path_root: Option<String>,
    auth: String,
}

impl AppAuthDefaultClient {
    /// Create a new App auth client using the given app key and secret, which can be found in the Dropbox app console.
    pub fn new(app_key: &str, app_secret: &str) -> Self {
        use base64::prelude::*;
        let encoded = BASE64_STANDARD.encode(format!("{app_key}:{app_secret}"));
        Self {
            inner: UreqClient::default(),
            path_root: None,
            auth: format!("Basic {encoded}"),
        }
    }

    impl_set_path_root!(self);
}

impl HttpClient for AppAuthDefaultClient {
    type Request = UreqRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        self.inner.execute(request, body)
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.inner
            .new_request(url)
            .set_header("Authorization", &self.auth)
    }
}

impl AppAuthClient for AppAuthDefaultClient {}

/// Default HTTP client for unauthenticated API calls.
#[derive(Debug, Default)]
pub struct NoauthDefaultClient {
    inner: UreqClient,
    path_root: Option<String>,
}

impl NoauthDefaultClient {
    impl_set_path_root!(self);
}

impl HttpClient for NoauthDefaultClient {
    type Request = UreqRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        self.inner.execute(request, body)
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.inner.new_request(url)
    }

    fn path_root(&self) -> Option<&str> {
        self.path_root.as_deref()
    }
}

impl NoauthClient for NoauthDefaultClient {}

/// Same as NoauthDefaultClient but with inner by reference and no path_root.
/// Only used for updating authorization tokens.
struct TokenUpdateClient<'a> {
    inner: &'a UreqClient,
}

impl HttpClient for TokenUpdateClient<'_> {
    type Request = UreqRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        self.inner.execute(request, body)
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.inner.new_request(url)
    }
}

impl crate::async_client_trait::NoauthClient for TokenUpdateClient<'_> {}

#[derive(Debug)]
struct UreqClient {
    agent: Agent,
}

impl Default for UreqClient {
    fn default() -> Self {
        Self {
            agent: Agent::new_with_config(
                Agent::config_builder()
                    .https_only(true)
                    .http_status_as_error(false)
                    .build(),
            ),
        }
    }
}

impl HttpClient for UreqClient {
    type Request = UreqRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        let resp = if body.is_empty() {
            request.req.send_empty()
        } else {
            request.req.send(body)
        };

        let (status, resp) = match resp {
            Ok(resp) => (resp.status().as_u16(), resp),
            Err(ureq::Error::Io(e)) => {
                return Err(e.into());
            }
            Err(e) => {
                return Err(RequestError { inner: e }.into());
            }
        };

        let result_header = resp
            .headers()
            .get("Dropbox-API-Result")
            .map(|v| String::from_utf8(v.as_bytes().to_vec()))
            .transpose()
            .map_err(|e| e.utf8_error())?;

        let content_length = resp
            .headers()
            .get("Content-Length")
            .map(|v| {
                let s = std::str::from_utf8(v.as_bytes())?;
                u64::from_str(s).map_err(|e| {
                    Error::UnexpectedResponse(format!("invalid Content-Length {s:?}: {e}"))
                })
            })
            .transpose()?;

        Ok(HttpRequestResultRaw {
            status,
            result_header,
            content_length,
            body: Box::new(resp.into_body().into_reader()),
        })
    }

    fn new_request(&self, url: &str) -> Self::Request {
        UreqRequest {
            req: self.agent.post(url),
        }
    }
}

/// This is an implementation detail of the HTTP client.
pub struct UreqRequest {
    req: ureq::RequestBuilder<WithBody>,
}

impl HttpRequest for UreqRequest {
    fn set_header(mut self, name: &str, value: &str) -> Self {
        self.req = self.req.header(name, value);
        self
    }
}

/// Errors from the HTTP client encountered in the course of making a request.
#[derive(thiserror::Error, Debug)]
#[allow(clippy::large_enum_variant)] // it's always boxed
pub enum DefaultClientError {
    /// The HTTP client encountered invalid UTF-8 data.
    #[error("invalid UTF-8 string")]
    Utf8(#[from] std::str::Utf8Error),

    /// The HTTP client encountered some I/O error.
    #[error("I/O error: {0}")]
    #[allow(clippy::upper_case_acronyms)]
    IO(#[from] std::io::Error),

    /// Some other error from the HTTP client implementation.
    #[error(transparent)]
    Request(#[from] RequestError),
}

macro_rules! wrap_error {
    ($e:ty) => {
        impl From<$e> for crate::Error {
            fn from(e: $e) -> Self {
                Self::HttpClient(Box::new(DefaultClientError::from(e)))
            }
        }
    };
}

wrap_error!(std::io::Error);
wrap_error!(std::str::Utf8Error);
wrap_error!(RequestError);

/// Something went wrong making the request, or the server returned a response we didn't expect.
/// Use the `Display` or `Debug` impls to see more details.
/// Note that this type is intentionally vague about the details beyond these string
/// representations, to allow implementation changes in the future.
pub struct RequestError {
    inner: ureq::Error,
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <ureq::Error as std::fmt::Display>::fmt(&self.inner, f)
    }
}

impl std::fmt::Debug for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <ureq::Error as std::fmt::Debug>::fmt(&self.inner, f)
    }
}

impl std::error::Error for RequestError {
    fn cause(&self) -> Option<&dyn std::error::Error> {
        Some(&self.inner)
    }
}
