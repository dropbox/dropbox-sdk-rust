//! The default async HTTP client.
//!
//! Use this client if you're not particularly picky about implementation details, as the specific
//! implementation is not exposed, and may be changed in the future.
//!
//! If you have a need for a specific HTTP client implementation, or your program is already using
//! some HTTP client crate, you probably want to have this Dropbox SDK crate use it as well. To do
//! that, you should implement the traits in `crate::client_trait` for it and use it instead.
//!
//! This code (and its dependencies) are only built if you use the `default_async_client` Cargo
//! feature.

use std::future::{Future, ready};
use std::str::FromStr;
use std::sync::Arc;
use bytes::Bytes;
use futures::{FutureExt, TryFutureExt, TryStreamExt};
use crate::async_client_trait::{HttpClient, HttpRequestResultRaw, NoauthClient, TeamAuthClient, UserAuthClient};
use crate::client_trait_common::{HttpRequest, TeamSelect};
use crate::default_client_common::impl_set_path_root;
use crate::oauth2::{Authorization, TokenCache};

macro_rules! impl_update_token {
    ($self:ident) => {
        fn update_token(&$self, old_token: Arc<String>)
            -> impl Future<Output = crate::Result<bool>> + Send
        {
            info!("refreshing auth token");
            $self.tokens
                .update_token(
                    TokenUpdateClient { inner: &$self.inner },
                    old_token,
                )
                .map(|r| match r {
                    Ok(_) => Ok(true),
                    Err(e) => {
                        error!("failed to update auth token: {e}");
                        Err(e.into())
                    }
                })
        }
    };
}

/// Default HTTP client using User authorization.
pub struct UserAuthDefaultClient {
    inner: ReqwestClient,
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
            inner: Default::default(),
            tokens,
            path_root: None,
        }
    }

    impl_set_path_root!(self);
}

impl HttpClient for UserAuthDefaultClient {
    type Request = ReqwestRequest;

    fn execute(
        &self,
        request: Self::Request,
        body: Bytes,
    ) -> impl Future<Output=crate::Result<HttpRequestResultRaw>> + Send {
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
    inner: ReqwestClient,
    tokens: Arc<TokenCache>,
    path_root: Option<String>, // a serialized PathRoot enum
    team_select: Option<TeamSelect>,
}

impl TeamAuthDefaultClient {
    /// Create a new client using the given OAuth2 token, with no user/admin context selected.
    pub fn new(tokens: impl Into<Arc<TokenCache>>) -> Self {
        Self {
            inner: Default::default(),
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
    type Request = ReqwestRequest;

    fn execute(
        &self,
        request: Self::Request,
        body: Bytes,
    ) -> impl Future<Output=crate::Result<HttpRequestResultRaw>> + Send {
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
    inner: ReqwestClient,
    path_root: Option<String>,
}

impl NoauthDefaultClient {
    impl_set_path_root!(self);
}

impl HttpClient for NoauthDefaultClient {
    type Request = ReqwestRequest;

    fn execute(
        &self,
        request: Self::Request,
        body: Bytes,
    ) -> impl Future<Output=crate::Result<HttpRequestResultRaw>> + Send {
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
    inner: &'a ReqwestClient,
}

impl<'a> HttpClient for TokenUpdateClient<'a> {
    type Request = ReqwestRequest;

    fn execute(
        &self,
        request: Self::Request,
        body: Bytes,
    ) -> impl Future<Output=crate::Result<HttpRequestResultRaw>> + Send {
        self.inner.execute(request, body)
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.inner.new_request(url)
    }
}

impl<'a> NoauthClient for TokenUpdateClient<'a> {}

#[derive(Debug)]
struct ReqwestClient {
    inner: reqwest::Client,
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self {
            inner: reqwest::Client::builder()
                .https_only(true)
                .http2_prior_knowledge()
                .build()
                .unwrap()
        }
    }
}

fn unexpected<T: std::error::Error + Send + Sync>(e: T, msg: &str) -> crate::Error {
    crate::Error::UnexpectedResponse(format!("{msg}: {e}"))
}

impl HttpClient for ReqwestClient {
    type Request = ReqwestRequest;

    fn execute(
        &self,
        request: Self::Request,
        body: Bytes,
    ) -> impl Future<Output = crate::Result<HttpRequestResultRaw>> + Send {
        let mut req = match request.req.build() {
            Ok(req) => req,
            Err(e) => {
                return ready(Err(crate::Error::HttpClient(Box::new(e)))).boxed();
            }
        };
        debug!("request for {}", req.url());
        if !body.is_empty() {
            *req.body_mut() = Some(reqwest::Body::from(body));
        }
        self.inner.execute(req)
            .map_ok_or_else(
                |e| Err(crate::Error::HttpClient(Box::new(e))),
                |resp| {
                    let status = resp.status().as_u16();

                    let result_header = resp
                        .headers()
                        .get("Dropbox-API-Result")
                        .map(|v| v.to_str())
                        .transpose()
                        .map_err(|e| unexpected(e, "invalid Dropbox-API-Result header"))?
                        .map(ToOwned::to_owned);

                    let content_length = resp
                        .headers()
                        .get("Content-Length")
                        .map(|v| {
                            v.to_str()
                                .map_err(|e| unexpected(e, "invalid Content-Length"))
                                .and_then(|s| {
                                    u64::from_str(s)
                                        .map_err(|e| unexpected(e, "invalid Content-Length"))
                                })
                        })
                        .transpose()?;

                    let body = resp.bytes_stream()
                        .map_err(|e| futures::io::Error::new(futures::io::ErrorKind::Other, e))
                        .into_async_read();

                    Ok(HttpRequestResultRaw {
                        status,
                        result_header,
                        content_length,
                        body: Box::new(body),
                    })
                }
            )
            .boxed()
    }

    fn new_request(&self, url: &str) -> Self::Request {
        ReqwestRequest {
            req: self.inner.post(url),
        }
    }
}

/// This is an implementation detail of the HTTP client.
pub struct ReqwestRequest {
    req: reqwest::RequestBuilder,
}

impl HttpRequest for ReqwestRequest {
    fn set_header(mut self, name: &str, value: &str) -> Self {
        self.req = self.req.header(name, value);
        self
    }
}
