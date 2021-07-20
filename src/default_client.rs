// Copyright (c) 2020-2021 Dropbox, Inc.

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

use crate::Error;
use crate::client_trait::*;
use crate::oauth2::{Authorization, TokenCache};
use std::sync::Arc;

const USER_AGENT: &str = concat!("Dropbox-APIv2-Rust/", env!("CARGO_PKG_VERSION"));

macro_rules! forward_noauth_request {
    ($self:ident, $inner:expr, $path_root:expr) => {
        fn request(
            &$self,
            endpoint: Endpoint,
            style: Style,
            function: &str,
            params: String,
            params_type: ParamsType,
            body: Option<&[u8]>,
            range_start: Option<u64>,
            range_end: Option<u64>,
        ) -> crate::Result<HttpRequestResultRaw> {
            $inner.request(endpoint, style, function, &params, params_type, body, range_start,
                range_end, None, $path_root, None)
        }
    }
}

macro_rules! forward_authed_request {
    ($self:ident, $tokens:expr, $inner:expr, $path_root:expr, $team_select:expr) => {
        fn request(
            &$self,
            endpoint: Endpoint,
            style: Style,
            function: &str,
            params: String,
            params_type: ParamsType,
            body: Option<&[u8]>,
            range_start: Option<u64>,
            range_end: Option<u64>,
        ) -> crate::Result<HttpRequestResultRaw> {
            let mut token = $tokens.get_token(TokenUpdateClient { inner: &$inner })?;

            let mut retried = false;
            loop {
                let result = $inner.request(endpoint, style, function, &params, params_type, body,
                    range_start, range_end, Some(&token), $path_root, $team_select);

                if retried {
                    break result;
                }

                if let Err(crate::Error::InvalidToken(msg)) = &result {
                    if msg == "expired_access_token" {
                        info!("refreshing token");
                        let old_token = token;
                        token = $tokens.update_token(
                            TokenUpdateClient { inner: &$inner },
                            old_token,
                        )?;
                        retried = true;
                        continue;
                    }
                }

                break result;
            }
        }
    }
}

macro_rules! impl_set_path_root {
    ($self:ident) => {
        /// Set a root which all subsequent paths are evaluated relative to.
        ///
        /// The default, if this function is not called, is to behave as if it was called with
        /// [`PathRoot::Home`](crate::common::PathRoot::Home).
        ///
        /// See <https://www.dropbox.com/developers/reference/path-root-header-modes> for more
        /// information.
        #[cfg(feature = "dbx_common")]
        pub fn set_path_root(&mut $self, path_root: &crate::common::PathRoot) {
            // Only way this can fail is if PathRoot::Other was specified, which is a programmer
            // error, so panic if that happens.
            $self.path_root = Some(serde_json::to_string(path_root).expect("invalid path root"));
        }
    }
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
    forward_authed_request! { self, self.tokens, self.inner, self.path_root.as_deref(), None }
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
    forward_authed_request! { self, self.tokens, self.inner, self.path_root.as_deref(), self.team_select.as_ref() }
}

impl TeamAuthClient for TeamAuthDefaultClient {}

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
    forward_noauth_request! { self, self.inner, self.path_root.as_deref() }
}

impl NoauthClient for NoauthDefaultClient {}

/// Same as NoauthDefaultClient but with inner by reference and no path_root.
/// Only used for updating authorization tokens.
struct TokenUpdateClient<'a> {
    inner: &'a UreqClient,
}

impl<'a> HttpClient for TokenUpdateClient<'a> {
    forward_noauth_request! { self, self.inner, None }
}

impl<'a> NoauthClient for TokenUpdateClient<'a> {}

#[derive(Debug, Default)]
struct UreqClient {}

impl UreqClient {
    #[allow(clippy::too_many_arguments)]
    fn request(
        &self,
        endpoint: Endpoint,
        style: Style,
        function: &str,
        params: &str,
        params_type: ParamsType,
        body: Option<&[u8]>,
        range_start: Option<u64>,
        range_end: Option<u64>,
        token: Option<&str>,
        path_root: Option<&str>,
        team_select: Option<&TeamSelect>,
    ) -> crate::Result<HttpRequestResultRaw> {

        let url = endpoint.url().to_owned() + function;
        debug!("request for {:?}", url);

        let mut req = ureq::post(&url)
            .set("User-Agent", USER_AGENT);

        if let Some(token) = token {
            req = req.set("Authorization", &format!("Bearer {}", token));
        }

        if let Some(path_root) = path_root {
            req = req.set("Dropbox-API-Path-Root", path_root);
        }

        if let Some(team_select) = team_select {
            req = match team_select {
                TeamSelect::User(id) => req.set("Dropbox-API-Select-User", id),
                TeamSelect::Admin(id) => req.set("Dropbox-API-Select-Admin", id),
            };
        }

        req = match (range_start, range_end) {
            (Some(start), Some(end)) => req.set("Range", &format!("bytes={}-{}", start, end)),
            (Some(start), None) => req.set("Range", &format!("bytes={}-", start)),
            (None, Some(end)) => req.set("Range", &format!("bytes=-{}", end)),
            (None, None) => req,
        };

        // If the params are totally empty, don't send any arg header or body.
        let result = if params.is_empty() {
            req.call()
        } else {
            match style {
                Style::Rpc => {
                    // Send params in the body.
                    req = req.set("Content-Type", params_type.content_type());
                    req.send_string(params)
                }
                Style::Upload | Style::Download => {
                    // Send params in a header.
                    req = req.set("Dropbox-API-Arg", params);
                    if style == Style::Upload {
                        req = req.set("Content-Type", "application/octet-stream");
                        if let Some(body) = body {
                            req.send_bytes(body)
                        } else {
                            req.send_bytes(&[])
                        }
                    } else {
                        assert!(body.is_none(), "body can only be set for Style::Upload request");
                        req.call()
                    }
                }
            }
        };

        let resp = match result {
            Ok(resp) => resp,
            Err(e @ ureq::Error::Transport(_)) => {
                error!("request failed: {}", e);
                return Err(RequestError { inner: e }.into());
            }
            Err(ureq::Error::Status(code, resp)) => {
                let status = resp.status_text().to_owned();
                let json = resp.into_string()?;
                return Err(Error::UnexpectedHttpError {
                    code,
                    status,
                    json,
                });
            }
        };

        match style {
            Style::Rpc | Style::Upload => {
                // Get the response from the body; return no body stream.
                let result_json = resp.into_string()?;
                Ok(HttpRequestResultRaw {
                    result_json,
                    content_length: None,
                    body: None,
                })
            }
            Style::Download => {
                // Get the response from a header; return the body stream.
                let result_json = resp.header("Dropbox-API-Result")
                    .ok_or(Error::UnexpectedResponse("missing Dropbox-API-Result header"))?
                    .to_owned();

                let content_length = match resp.header("Content-Length") {
                    Some(s) => Some(s.parse()
                        .map_err(|_| Error::UnexpectedResponse("invalid Content-Length header"))?),
                    None => None,
                };

                Ok(HttpRequestResultRaw {
                    result_json,
                    content_length,
                    body: Some(Box::new(resp.into_reader())),
                })
            }
        }
    }
}

/// Errors from the HTTP client encountered in the course of making a request.
#[derive(thiserror::Error, Debug)]
#[allow(clippy::large_enum_variant)] // it's always boxed
pub enum DefaultClientError {
    /// The HTTP client encountered invalid UTF-8 data.
    #[error("invalid UTF-8 string")]
    Utf8(#[from] std::string::FromUtf8Error),

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
    }
}

wrap_error!(std::io::Error);
wrap_error!(std::string::FromUtf8Error);
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
