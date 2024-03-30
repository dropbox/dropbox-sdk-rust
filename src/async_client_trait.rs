//! Everything needed to implement your async HTTP client.

use std::future::{Future, ready};
use std::io::{IoSliceMut, Read};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use futures::{AsyncRead, FutureExt};
use crate::client_trait as sync;
use crate::client_trait_common::{HttpRequest, TeamSelect};

/// The base HTTP asynchronous client trait.
pub trait HttpClient {
    /// The concrete type of request supported by the client.
    type Request: HttpRequest;

    /// Make a HTTP request.
    fn execute(
        &self,
        request: Self::Request,
    ) -> impl Future<Output = crate::Result<HttpRequestResultRaw>> + Send;

    /// Create a new request instance for the given URL. It should be a POST request.
    fn new_request(&self, url: &str) -> Self::Request;

    /// Attempt to update the current authentication token. The previously fetched token is given
    /// as a way to avoid repeat updates in case of a race. If the update is successful, return
    /// `true` and the current request will be retried with a newly-fetched token. Return `false` if
    /// authentication is not supported, or if the update operation fails.
    fn update_token(
        &self,
        _old_token: Arc<String>,
    ) -> impl Future<Output = bool> {
        ready(false).boxed()
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

/// The raw response from the server, including an async streaming response body.
pub struct HttpRequestResultRaw {
    /// HTTP response code and message.
    pub status: (u16, String),

    /// The value of the `Dropbox-API-Result` header, if present.
    pub result_header: Option<String>,

    /// The value of the `Content-Length` header, if present.
    pub content_length: Option<u64>,

    /// The response body stream.
    pub body: Box<dyn AsyncRead + Unpin + Send>,
}

/// The response from the server, parsed into a given type, including a body stream if it is from
/// a Download style request.
pub struct HttpRequestResult<T> {
    /// The API result, parsed into the given type.
    pub result: T,

    /// The value of the `Content-Length` header in the response, if any. Only expected to not be
    /// `None` if `body` is also not `None`.
    pub content_length: Option<u64>,

    /// The response body stream, if any. Only expected to not be `None` for [`Style::Download`]
    /// endpoints.
    pub body: Option<Box<dyn AsyncRead + Unpin + Send>>,
}

/// Blanket implementation of the async interface for all sync clients.
/// This is necessary because all the machinery is actually implemented in terms of the async
/// client.
impl<T: sync::HttpClient> HttpClient for T {
    type Request = T::Request;

    fn execute(&self, request: Self::Request) -> impl Future<Output=crate::Result<HttpRequestResultRaw>> + Send {
        ready(self.execute(request).map(|r| {
            HttpRequestResultRaw {
                status: r.status,
                result_header: r.result_header,
                content_length: r.content_length,
                body: Box::new(SyncReadAdapter { inner: r.body }),
            }
        }))
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.new_request(url)
    }

    fn update_token(&self, old_token: Arc<String>) -> impl Future<Output=bool> {
        ready(self.update_token(old_token))
    }

    fn token(&self) -> Option<Arc<String>> {
        self.token()
    }

    fn path_root(&self) -> Option<&str> {
        self.path_root()
    }

    fn team_select(&self) -> Option<&TeamSelect> {
        self.team_select()
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

pub(crate) struct SyncReadAdapter {
    pub inner: Box<dyn Read + Send>,
}

impl AsyncRead for SyncReadAdapter {
    fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        Poll::Ready(self.inner.read(buf))
    }

    fn poll_read_vectored(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, bufs: &mut [IoSliceMut<'_>]) -> Poll<std::io::Result<usize>> {
        Poll::Ready(self.inner.read_vectored(bufs))
    }
}
