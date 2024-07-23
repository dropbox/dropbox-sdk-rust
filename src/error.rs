use crate::types;

/// An error occurred in the process of making an API call.
/// This is different from the case where your call succeeded, but the operation returned an error.
#[derive(thiserror::Error, Debug)]
pub enum Error<E = NoError> {
    /// An error returned by the API. Its type depends on the endpoint being called.
    #[error("Dropbox API endpoint returned an error: {0}")]
    Api(#[source] E),

    /// Some error from the internals of the HTTP client.
    #[error("error from HTTP client: {0}")]
    HttpClient(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),

    /// Something went wrong in the process of transforming your arguments into a JSON string.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// The Dropbox API response was unexpected or malformed in some way.
    #[error("Dropbox API returned something unexpected: {0}")]
    UnexpectedResponse(String),

    /// The Dropbox API indicated that your request was malformed in some way.
    #[error("Dropbox API indicated that the request was malformed: {0}")]
    BadRequest(String),

    /// Errors occurred during authentication.
    #[error("Dropbox API indicated a problem with authentication: {0}")]
    Authentication(#[source] types::auth::AuthError),

    /// Your request was rejected due to rate-limiting. You can retry it later.
    #[error("Dropbox API declined the request due to rate-limiting ({reason}), \
        retry after {retry_after_seconds}s")]
    RateLimited {
        /// The server-given reason for the rate-limiting.
        reason: types::auth::RateLimitReason,

        /// You can retry this request after this many seconds.
        retry_after_seconds: u32,
    },

    /// The user or team account doesn't have access to the endpoint or feature.
    #[error("Dropbox API denied access to the resource: {0}")]
    AccessDenied(#[source] types::auth::AccessError),

    /// The Dropbox API server had an internal error.
    #[error("Dropbox API had an internal server error: {0}")]
    ServerError(String),

    /// The Dropbox API returned an unexpected HTTP response code.
    #[error("Dropbox API returned HTTP {code} - {response}")]
    UnexpectedHttpError {
        /// HTTP status code returned.
        code: u16,

        /// The response body.
        response: String,
    },
}

/// An [`Error`] without a single concrete type for the API error response, using a boxed trait
/// object instead.
///
/// This is useful if a function needs to return some combination of different error types. They
/// can be extracted later by using
/// [`std::error::Error::downcast_ref`](https://doc.rust-lang.org/std/error/trait.Error.html#method.downcast_ref)
/// or [`Error::downcast_ref_inner`] if desired.
///
/// See [`Error::boxed`] for how to convert a concretely-typed version of [`Error`] into this.
pub type BoxedError = Error<Box<dyn std::error::Error>>;

impl<E: std::error::Error + 'static> Error<E> {
    /// Look for an inner error of the given type anywhere within this error, by walking the chain
    /// of [`std::error::Error::source`] recursively until something matches the desired type.
    pub fn downcast_ref_inner<E2: std::error::Error + 'static>(&self) -> Option<&E2> {
        let mut inner = Some(self as &dyn std::error::Error);
        while let Some(e) = inner {
            if let Some(e) = e.downcast_ref() {
                return Some(e);
            }
            inner = e.source();
        }
        None
    }

    /// Change the concretely-typed API error, if any, into a boxed trait object.
    ///
    /// This makes it possible to combine dissimilar errors into one type, which can be broken out
    /// later using
    /// [`std::error::Error::downcast_ref`](https://doc.rust-lang.org/std/error/trait.Error.html#method.downcast_ref)
    /// if desired.
    pub fn boxed(self) -> BoxedError {
        match self {
            Error::Api(e) => Error::Api(Box::new(e)),

            // Other variants unchanged.
            // These have to be actually re-stated, because the (unstated) generic type of `Error`
            // is different on the left vs the right.
            Error::HttpClient(e) => Error::HttpClient(e),
            Error::Json(e) => Error::Json(e),
            Error::UnexpectedResponse(e) => Error::UnexpectedResponse(e),
            Error::BadRequest(e) => Error::BadRequest(e),
            Error::Authentication(e) => Error::Authentication(e),
            Error::RateLimited { reason, retry_after_seconds } => Error::RateLimited { reason, retry_after_seconds },
            Error::AccessDenied(e) => Error::AccessDenied(e),
            Error::ServerError(e) => Error::ServerError(e),
            Error::UnexpectedHttpError { code, response } => Error::UnexpectedHttpError { code, response },
        }
    }
}

impl Error<NoError> {
    /// Lift an error with no possible API error value to a typed error of any type.
    ///
    /// Ideally this would just be `impl<E> From<Error<NoError>> for Error<E>` but that conflicts
    /// with the reflexive conversion (E could be NoError), and Rust doesn't have negative type
    /// bounds or specialization, so it has to be this method instead.
    pub fn typed<E>(self) -> Error<E> {
        match self {
            Error::Api(x) => unreachable(x),
            Error::HttpClient(e) => Error::HttpClient(e),
            Error::Json(e) => Error::Json(e),
            Error::UnexpectedResponse(e) => Error::UnexpectedResponse(e),
            Error::BadRequest(e) => Error::BadRequest(e),
            Error::Authentication(e) => Error::Authentication(e),
            Error::RateLimited { reason, retry_after_seconds } => Error::RateLimited { reason, retry_after_seconds },
            Error::AccessDenied(e) => Error::AccessDenied(e),
            Error::ServerError(e) => Error::ServerError(e),
            Error::UnexpectedHttpError { code, response } => Error::UnexpectedHttpError { code, response },
        }
    }
}


/// A special error type for a method that doesn't have any defined error return. You can't
/// actually encounter a value of this type in real life; it's here to satisfy type requirements.
#[derive(Copy, Clone)]
pub enum NoError {}

impl PartialEq<NoError> for NoError {
    fn eq(&self, _: &NoError) -> bool {
        unreachable(*self)
    }
}

impl std::error::Error for NoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        unreachable(*self)
    }

    fn description(&self) -> &str {
        unreachable(*self)
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        unreachable(*self)
    }
}

impl std::fmt::Debug for NoError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unreachable(*self)
    }
}

impl std::fmt::Display for NoError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unreachable(*self)
    }
}

// This is the reason we can't just use the otherwise-identical `void` crate's Void type: we need
// to implement this trait.
impl<'de> serde::de::Deserialize<'de> for NoError {
    fn deserialize<D: serde::de::Deserializer<'de>>(_: D)
        -> Result<Self, D::Error>
    {
        Err(serde::de::Error::custom(
            "method has no defined error type, but an error was returned"))
    }
}

#[inline(always)]
fn unreachable(x: NoError) -> ! {
    match x {}
}
