// Copyright (c) 2019-2020 Dropbox, Inc.

use std::io::{self, Read};
use std::str;

use crate::Error;
use crate::client_trait::{Endpoint, Style, HttpClient, HttpRequestResultRaw, ParamsType,
    TeamAuthClient, TeamSelect, UserAuthClient, NoauthClient};
use hyper::{self, Url};
use hyper::header::Headers;
use hyper::header::{
    Authorization, Bearer, ByteRangeSpec, Connection, ContentLength, ContentType, Range};

const USER_AGENT: &str = concat!("Dropbox-APIv2-Rust/", env!("CARGO_PKG_VERSION"));

macro_rules! forward_request {
    { $self:ident, client: $client:expr, token: $token:expr, team_select: $team_select:expr } => {
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
            $client.request(endpoint, style, function, params, params_type, body, range_start,
                range_end, $token, $team_select)
        }
    }
}

// Noauth client:

#[derive(Default)]
pub struct NoauthHyperClient {
    inner: HyperClient,
}

impl HttpClient for NoauthHyperClient {
    forward_request! { self, client: self.inner, token: None, team_select: None }
}

impl NoauthClient for NoauthHyperClient {}

// User auth client:

pub struct UserAuthHyperClient {
    inner: HyperClient,
    token: String,
}

impl UserAuthHyperClient {
    pub fn new(token: String) -> Self {
        Self {
            inner: HyperClient::default(),
            token,
        }
    }
}

impl HttpClient for UserAuthHyperClient {
    forward_request! { self, client: self.inner, token: Some(&self.token), team_select: None }
}

impl UserAuthClient for UserAuthHyperClient {}

// Team auth client:

pub struct TeamAuthHyperClient {
    inner: HyperClient,
    token: String,
    team_select: Option<TeamSelect>,
}

impl TeamAuthHyperClient {
    pub fn new(token: String) -> Self {
        Self {
            inner: HyperClient::default(),
            token,
            team_select: None,
        }
    }

    pub fn select(&mut self, team_select: Option<TeamSelect>) {
        self.team_select = team_select;
    }
}

impl HttpClient for TeamAuthHyperClient {
    forward_request! { self, client: self.inner, token: Some(&self.token),
        team_select: self.team_select.as_ref() }
}

impl TeamAuthClient for TeamAuthHyperClient {}

// Errors:

#[derive(thiserror::Error, Debug)]
pub enum HyperClientError {
    #[error("Invalid UTF-8 string")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Hyper(#[from] hyper::Error),
}

// Implement From for some errors so that they get wrapped in a HyperClientError and then
// propogated via Error::HttpClient. Note that this only works for types that don't already have a
// variant in the crate Error type, because doing so would produce a conflicting impl.
macro_rules! hyper_error {
    ($e:ty) => {
        impl From<$e> for crate::Error {
            fn from(e: $e) -> Self {
                Self::HttpClient(Box::new(HyperClientError::from(e)))
            }
        }
    }
}

hyper_error!(std::io::Error);
hyper_error!(std::string::FromUtf8Error);
hyper_error!(hyper::Error);

// Common HTTP client:

fn http_client() -> hyper::client::Client {
    let tls = hyper_native_tls::NativeTlsClient::new().unwrap();
    let https_connector = hyper::net::HttpsConnector::new(tls);
    let pool_connector = hyper::client::pool::Pool::with_connector(
        hyper::client::pool::Config { max_idle: 1 },
        https_connector);
    hyper::client::Client::with_connector(pool_connector)
}

struct HyperClient {
    client: hyper::client::Client,
}

impl Default for HyperClient {
    fn default() -> Self {
        Self {
            client: http_client(),
        }
    }
}

impl HyperClient {
    #[allow(clippy::too_many_arguments)]
    pub fn request(
        &self,
        endpoint: Endpoint,
        style: Style,
        function: &str,
        params: String,
        params_type: ParamsType,
        body: Option<&[u8]>,
        range_start: Option<u64>,
        range_end: Option<u64>,
        token: Option<&str>,
        team_select: Option<&TeamSelect>,
    ) -> crate::Result<HttpRequestResultRaw> {

        let url = Url::parse(endpoint.url()).unwrap().join(function).expect("invalid request URL");
        debug!("request for {:?}", url);

        loop {
            let mut builder = self.client.post(url.clone());

            let mut headers = Headers::new();
            headers.set(UserAgent(USER_AGENT));
            if let Some(token) = token {
                headers.set(Authorization(Bearer { token: token.to_owned() }));
            }
            if let Some(team_select) = team_select {
                let value = match team_select {
                    TeamSelect::User(id) => id,
                    TeamSelect::Admin(id) => id,
                };
                headers.set_raw(team_select.header_name(), vec![value.to_owned().into_bytes()]);
            }
            headers.set(Connection::keep_alive());

            if let Some(start) = range_start {
                if let Some(end) = range_end {
                    headers.set(Range::Bytes(vec![ByteRangeSpec::FromTo(start, end)]));
                } else {
                    headers.set(Range::Bytes(vec![ByteRangeSpec::AllFrom(start)]));
                }
            } else if let Some(end) = range_end {
                headers.set(Range::Bytes(vec![ByteRangeSpec::Last(end)]));
            }

            // If the params are totally empty, don't send any arg header or body.
            if !params.is_empty() {
                match style {
                    Style::Rpc => {
                        // Send params in the body.
                        match params_type {
                            ParamsType::Json => headers.set(ContentType::json()),
                            ParamsType::Form => headers.set(ContentType::form_url_encoded()),
                        };
                        builder = builder.body(params.as_bytes());
                        assert_eq!(None, body);
                    },
                    Style::Upload | Style::Download => {
                        // Send params in a header.
                        headers.set_raw("Dropbox-API-Arg", vec![params.clone().into_bytes()]);
                        if style == Style::Upload {
                            headers.set(
                                ContentType(
                                    hyper::mime::Mime(
                                        hyper::mime::TopLevel::Application,
                                        hyper::mime::SubLevel::OctetStream,
                                        vec![])));
                        }
                        if let Some(body) = body {
                            builder = builder.body(body);
                        }
                    }
                }
            }

            let mut resp = match builder.headers(headers).send() {
                Ok(resp) => resp,
                Err(hyper::error::Error::Io(ref ioerr))
                        if ioerr.kind() == io::ErrorKind::ConnectionAborted => {
                    debug!("connection closed; retrying...");
                    continue;
                },
                Err(other) => {
                    error!("request failed: {}", other);
                    return Err(other.into());
                }
            };

            if !resp.status.is_success() {
                let hyper::http::RawStatus(code, status) = resp.status_raw().clone();
                let mut json = String::new();
                resp.read_to_string(&mut json)?;
                return Err(Error::UnexpectedHttpError {
                    code,
                    status: status.into_owned(),
                    json,
                });
            }

            return match style {
                Style::Rpc | Style::Upload => {
                    // Get the response from the body; return no body stream.
                    let mut s = String::new();
                    resp.read_to_string(&mut s)?;
                    Ok(HttpRequestResultRaw {
                        result_json: s,
                        content_length: None,
                        body: None,
                    })
                },
                Style::Download => {
                    // Get the response from a header; return the body stream.
                    let s = match resp.headers.get_raw("Dropbox-API-Result") {
                        Some(values) => {
                            String::from_utf8(values[0].clone())?
                        },
                        None => {
                            return Err(Error::UnexpectedResponse(
                                "missing Dropbox-API-Result header"));
                        }
                    };

                    let len = resp.headers.get::<ContentLength>().map(|h| h.0);

                    Ok(HttpRequestResultRaw {
                        result_json: s,
                        content_length: len,
                        body: Some(Box::new(resp)),
                    })
                }
            }

        }
    }
}

#[derive(Debug, Copy, Clone)]
struct UserAgent(&'static str);
impl hyper::header::Header for UserAgent {
    fn header_name() -> &'static str { "User-Agent" }
    fn parse_header(_: &[Vec<u8>]) -> Result<Self, hyper::Error> { unimplemented!() }
}
impl hyper::header::HeaderFormat for UserAgent {
    fn fmt_header(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        f.write_str(self.0)
    }
}
