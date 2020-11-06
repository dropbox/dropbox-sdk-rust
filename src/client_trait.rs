// Copyright (c) 2019-2020 Dropbox, Inc.

//! Everything needed to implement your HTTP client.

use std::io::Read;

/// The base HTTP client trait.
pub trait HttpClient {
    /// Make a HTTP request.
    #[allow(clippy::too_many_arguments)]
    fn request(
        &self,
        endpoint: Endpoint,
        style: Style,
        function: &str,
        params: String,
        params_type: ParamsType,
        body: Option<&[u8]>,
        range_start: Option<u64>,
        range_end: Option<u64>,
    ) -> crate::Result<HttpRequestResultRaw>;
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

/// The raw response from the server, containing the result from either a header or the body, as
/// appropriate to the request style, and a body stream if it is from a Download style request.
pub struct HttpRequestResultRaw {
    /// The response body JSON string.
    pub result_json: String,

    /// The value of the `Content-Length` header in the response, if any. Only expected to not be
    /// `None` if `body` is also not `None`.
    pub content_length: Option<u64>,

    /// The response body stream, if any. Only expected to not be `None` for [`Style::Download`]
    /// endpoints.
    pub body: Option<Box<dyn Read>>,
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
    pub body: Option<Box<dyn Read>>,
}

/// The API base endpoint for a request. Determines which hostname the request should go to.
#[derive(Debug, Copy, Clone, PartialEq)]
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
#[derive(Debug, Copy, Clone, PartialEq)]
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
#[derive(Debug, Copy, Clone, PartialEq)]
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
