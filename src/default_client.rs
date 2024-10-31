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
use crate::oauth2::{Authorization, TokenCache};
use std::borrow::Cow;
use std::fmt::Write;
use std::str::FromStr;
use std::sync::Arc;
use futures::FutureExt;
use crate::client_trait::{HttpClient, HttpRequestResultRaw, NoauthClient, TeamAuthClient, UserAuthClient};
use crate::client_trait_common::{HttpRequest, TeamSelect};
use crate::default_client_common::impl_set_path_root;

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

impl<'a> HttpClient for TokenUpdateClient<'a> {
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
    agent: ureq::Agent,
}

impl Default for UreqClient {
    fn default() -> Self {
        Self {
            agent: ureq::Agent::new(),
        }
    }
}

impl HttpClient for UreqClient {
    type Request = UreqRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        let resp = if body.is_empty() {
            request.req.call()
        } else {
            request.req.send_bytes(body)
        };

        let (status, resp) = match resp {
            Ok(resp) => {
                (resp.status(), resp)
            }
            Err(ureq::Error::Status(status, resp)) => {
                (status, resp)
            }
            Err(e @ ureq::Error::Transport(_)) => {
                return Err(RequestError { inner: e }.into());
            }
        };

        let result_header = resp.header("Dropbox-API-Result").map(String::from);

        let content_length = resp.header("Content-Length")
            .map(|s| {
                u64::from_str(s)
                    .map_err(|e| Error::UnexpectedResponse(
                        format!("invalid Content-Length {s:?}: {e}")))
            })
            .transpose()?;

        Ok(HttpRequestResultRaw {
            status,
            result_header,
            content_length,
            body: resp.into_reader(),
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
    req: ureq::Request,
}

impl HttpRequest for UreqRequest {
    fn set_header(mut self, name: &str, value: &str) -> Self {
        if name.eq_ignore_ascii_case("dropbox-api-arg") {
            // Non-ASCII and 0x7F in a header need to be escaped per the HTTP spec, and ureq doesn't
            // do this for us. This is only an issue for this particular header.
            self.req = self.req.set(name, json_escape_header(value).as_ref());
        } else {
            self.req = self.req.set(name, value);
        }
        self
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

/// Replaces any non-ASCII characters (and 0x7f) with JSON-style '\uXXXX' sequence. Otherwise,
/// returns it unmodified without any additional allocation or copying.
fn json_escape_header(s: &str) -> Cow<'_, str> {
    // Unfortunately, the HTTP spec requires escaping ASCII DEL (0x7F), so we can't use the quicker
    // bit pattern check done in str::is_ascii() to skip this for the common case of all ASCII. :(

    let mut out = Cow::Borrowed(s);
    for (i, c) in s.char_indices() {
        if !c.is_ascii() || c == '\x7f' {
            let mstr = match out {
                Cow::Borrowed(_) => {
                    // If we're still borrowed, we must have had ascii up until this point.
                    // Clone the string up until here, and from now on we'll be pushing chars to it.
                    out = Cow::Owned(s[0..i].to_owned());
                    out.to_mut()
                }
                Cow::Owned(ref mut m) => m,
            };
            write!(mstr, "\\u{:04x}", c as u32).unwrap();
        } else if let Cow::Owned(ref mut o) = out {
            o.push(c);
        }
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_json_escape() {
        assert_eq!(Cow::Borrowed("foobar"), json_escape_header("foobar"));
        assert_eq!(
            Cow::<'_, str>::Owned("tro\\u0161kovi".to_owned()),
            json_escape_header("troškovi"));
        assert_eq!(
            Cow::<'_, str>::Owned(
                r#"{"field": "some_\u00fc\u00f1\u00eec\u00f8d\u00e9_and_\u007f"}"#.to_owned()),
            json_escape_header("{\"field\": \"some_üñîcødé_and_\x7f\"}"));
        assert_eq!(
            Cow::<'_, str>::Owned("almost,\\u007f but not quite".to_owned()),
            json_escape_header("almost,\x7f but not quite"));
    }
}
