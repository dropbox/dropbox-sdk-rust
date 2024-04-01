//! Everything needed to implement your async HTTP client.

use std::future::{Future, ready};
use std::sync::Arc;
use bytes::Bytes;
use futures::AsyncRead;
use crate::client_trait_common::{HttpRequest, TeamSelect};

/// The base HTTP asynchronous client trait.
pub trait HttpClient: Sync {
    /// The concrete type of request supported by the client.
    type Request: HttpRequest + Send;

    /// Make a HTTP request.
    fn execute(
        &self,
        request: Self::Request,
        body: Bytes,
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
    ) -> impl Future<Output = bool> + Send {
        ready(false)
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

    /// This should only be implemented by (or called on) the blanket impl for sync HTTP clients
    /// implemented in this module.
    ///
    /// It's necessary because
    ///   * there's no efficient way to implement an async client which takes a request body slice
    ///     (making a Bytes involves a copy)
    ///   * there IS a way to do it for sync clients
    ///   * the signature of the sync upload routes takes the body this way
    ///   * we don't want to break compatibility
    ///
    /// Only the sync routes take a body arg this way, and this logic only gets invoked for those,
    /// so only the sync HTTP client wrapper needs to implement it.
    #[doc(hidden)]
    #[cfg(feature = "sync_routes")]
    fn execute_borrowed_body(
        &self,
        _request: Self::Request,
        _body_slice: &[u8],
    ) -> impl Future<Output = crate::Result<HttpRequestResultRaw>> + Send {
        unimplemented!();
        #[allow(unreachable_code)] // otherwise it complains that `()` is not a future.
        async move { unimplemented!() }
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
    pub body: Box<dyn AsyncRead + Send + Unpin>,
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
#[cfg(feature = "sync_routes")]
impl<T: crate::client_trait::HttpClient + Sync> HttpClient for T {
    type Request = T::Request;

    async fn execute(&self, request: Self::Request, body: Bytes) -> crate::Result<HttpRequestResultRaw> {
        self.execute_borrowed_body(request, &body).await
    }

    async fn execute_borrowed_body(&self, request: Self::Request, body_slice: &[u8]) -> crate::Result<HttpRequestResultRaw> {
        self.execute(request, body_slice).map(|r| {
            HttpRequestResultRaw {
                status: r.status,
                result_header: r.result_header,
                content_length: r.content_length,
                body: Box::new(SyncReadAdapter { inner: r.body }),
            }
        })
    }

    fn new_request(&self, url: &str) -> Self::Request {
        self.new_request(url)
    }

    fn update_token(&self, old_token: Arc<String>) -> impl Future<Output=bool> + Send {
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

#[cfg(feature = "sync_routes")]
pub(crate) struct SyncReadAdapter {
    pub inner: Box<dyn std::io::Read + Send>,
}

#[cfg(feature = "sync_routes")]
impl AsyncRead for SyncReadAdapter {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(std::io::Read::read(&mut self.inner, buf))
    }

    fn poll_read_vectored(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        bufs: &mut [std::io::IoSliceMut<'_>],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(std::io::Read::read_vectored(&mut self.inner, bufs))
    }
}
