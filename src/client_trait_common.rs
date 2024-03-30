//! Types common to the sync and async HTTP clients.

use bytes::Bytes;

/// A builder for a HTTP request.
pub trait HttpRequest {
    /// Set a HTTP header.
    fn set_header(self, name: &str, value: &str) -> Self;

    /// Set the request body.
    fn set_body(self, body: Bytes) -> Self;
}

/// The API base endpoint for a request. Determines which hostname the request should go to.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// The endpoint used for most API calls.
    Api,

    /// The endpoint primarily used for upload and download calls.
    Content,

    /// The endpoint primarily used for longpolling calls.
    Notify,

    /// The endpoint used for OAuth2 token requests.
    OAuth2,
}

impl Endpoint {
    /// The base URL for API calls using the given endpoint.
    pub fn url(self) -> &'static str {
        match self {
            Endpoint::Api => "https://api.dropboxapi.com/2/",
            Endpoint::Content => "https://content.dropboxapi.com/2/",
            Endpoint::Notify => "https://notify.dropboxapi.com/2/",
            Endpoint::OAuth2 => "https://api.dropboxapi.com/", // note no '2/'
        }
    }
}

/// The style of a request, which determines how arguments are passed, and whether there is a
/// request and/or response body.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Style {
    /// Arguments are passed in the request body; response is in the body; no request or response
    /// body content stream.
    Rpc,

    /// Arguments are passed in a HTTP header; response is in the body; request body is the upload
    /// content; no response body content stream.
    Upload,

    /// Arguments are passed in a HTTP header; response is in a HTTP header; no request content
    /// body; response body contains the content stream.
    Download,
}

/// The format of arguments being sent in a request.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ParamsType {
    /// JSON.
    Json,

    /// WWW Form URL-encoded. Only used for OAuth2 requests.
    Form,
}

impl ParamsType {
    /// The value for the HTTP Content-Type header for the given params format.
    pub fn content_type(self) -> &'static str {
        match self {
            ParamsType::Json => "application/json",
            ParamsType::Form => "application/x-www-form-urlencoded",
        }
    }
}

/// Used with Team Authentication to select a user context within that team.
#[derive(Debug, Clone)]
pub enum TeamSelect {
    /// A team member's user ID.
    User(String),

    /// A team admin's user ID, which grants additional access.
    Admin(String),
}

impl TeamSelect {
    /// The name of the HTTP header that must be set.
    pub fn header_name(&self) -> &'static str {
        match self {
            TeamSelect::User(_) => "Dropbox-API-Select-User",
            TeamSelect::Admin(_) => "Dropbox-API-Select-Admin",
        }
    }
}
