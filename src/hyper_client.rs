use std::io::{self, Read};
use std::str;

use crate::ErrorKind;
use crate::client_trait::{Endpoint, Style, HttpClient, HttpRequestResultRaw};
use hyper::{self, Url};
use hyper::header::Headers;
use hyper::header::{
    Authorization, Bearer, ByteRangeSpec, Connection, ContentLength, ContentType, Range};
use hyper_native_tls;
use serde_json;
use url::form_urlencoded::Serializer as UrlEncoder;

const USER_AGENT: &str = concat!("Dropbox-APIv2-Rust/", env!("CARGO_PKG_VERSION"));

pub struct HyperClient {
    client: hyper::client::Client,
    token: String,
}

impl HyperClient {
    pub fn new(token: String) -> HyperClient {
        HyperClient {
            client: Self::http_client(),
            token,
        }
    }

    /// Given an authorization code, request an OAuth2 token from Dropbox API.
    /// Requires the App ID and secret, as well as the redirect URI used in the prior authorize
    /// request, if there was one.
    pub fn oauth2_token_from_authorization_code(
        client_id: &str,
        client_secret: &str,
        authorization_code: &str,
        redirect_uri: Option<&str>,
    ) -> super::Result<String> {

        let client = Self::http_client();
        let url = Url::parse("https://api.dropboxapi.com/oauth2/token").unwrap();

        let mut headers = Headers::new();
        headers.set(UserAgent(USER_AGENT));

        // This endpoint wants parameters using URL-encoding instead of JSON.
        headers.set(ContentType("application/x-www-form-urlencoded".parse().unwrap()));
        let mut params = UrlEncoder::new(String::new());
        params.append_pair("code", authorization_code);
        params.append_pair("grant_type", "authorization_code");
        params.append_pair("client_id", client_id);
        params.append_pair("client_secret", client_secret);
        if let Some(value) = redirect_uri {
            params.append_pair("redirect_uri", value);
        }
        let body = params.finish();

        match client.post(url).headers(headers).body(body.as_bytes()).send() {
            Ok(mut resp) => {
                if !resp.status.is_success() {
                    let (code, status) = {
                        let &hyper::http::RawStatus(ref code, ref status) = resp.status_raw();
                        use std::ops::Deref;
                        (*code, status.deref().to_owned())
                    };
                    let mut body = String::new();
                    resp.read_to_string(&mut body)?;
                    debug!("error body: {}", body);
                    Err(ErrorKind::GeneralHttpError(code, status, body).into())
                } else {
                    let body = serde_json::from_reader(resp)?;
                    debug!("response: {:?}", body);
                    match body {
                        serde_json::Value::Object(mut map) => {
                            match map.remove("access_token") {
                                Some(serde_json::Value::String(token)) => Ok(token),
                                _ => bail!("no access token in response!")
                            }
                        },
                        _ => bail!("invalid response from server")
                    }
                }
            },
            Err(e) => {
                error!("error getting OAuth2 token: {}", e);
                Err(e.into())
            }
        }
    }

    fn http_client() -> hyper::client::Client {
        let tls = hyper_native_tls::NativeTlsClient::new().unwrap();
        let https_connector = hyper::net::HttpsConnector::new(tls);
        let pool_connector = hyper::client::pool::Pool::with_connector(
            hyper::client::pool::Config { max_idle: 1 },
            https_connector);
        hyper::client::Client::with_connector(pool_connector)
    }
}

impl HttpClient for HyperClient {
    fn request(
        &self,
        endpoint: Endpoint,
        style: Style,
        function: &str,
        params_json: String,
        body: Option<&[u8]>,
        range_start: Option<u64>,
        range_end: Option<u64>,
    ) -> super::Result<HttpRequestResultRaw> {

        let url = Url::parse(endpoint.url()).unwrap().join(function).expect("invalid request URL");
        debug!("request for {:?}", url);

        loop {
            let mut builder = self.client.post(url.clone());

            let mut headers = Headers::new();
            headers.set(UserAgent(USER_AGENT));
            headers.set(Authorization(Bearer { token: self.token.clone() }));
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

            // If the params are totally empt, don't send any arg header or body.
            if !params_json.is_empty() {
                match style {
                    Style::Rpc => {
                        // Send params in the body.
                        headers.set(ContentType::json());
                        builder = builder.body(params_json.as_bytes());
                        assert_eq!(None, body);
                    },
                    Style::Upload | Style::Download => {
                        // Send params in a header.
                        headers.set_raw("Dropbox-API-Arg", vec![params_json.clone().into_bytes()]);
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
                let (code, status) = {
                    let &hyper::http::RawStatus(ref code, ref status) = resp.status_raw();
                    use std::ops::Deref;
                    (*code, status.deref().to_owned())
                };
                let mut json = String::new();
                resp.read_to_string(&mut json)?;
                return Err(ErrorKind::GeneralHttpError(code, status, json).into());
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
                            bail!(ErrorKind::UnexpectedError("missing Dropbox-API-Result header"));
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

/// Builds a URL that can be given to the user to visit to have Dropbox authorize your app.
#[derive(Debug)]
pub struct Oauth2AuthorizeUrlBuilder<'a> {
    client_id: &'a str,
    response_type: &'a str,
    force_reapprove: bool,
    force_reauthentication: bool,
    disable_signup: bool,
    redirect_uri: Option<&'a str>,
    state: Option<&'a str>,
    require_role: Option<&'a str>,
    locale: Option<&'a str>,
}

/// Which type of OAuth2 flow to use.
#[derive(Debug, Copy, Clone)]
pub enum Oauth2Type {
    /// Authorization yields a temporary authorization code which must be turned into an OAuth2
    /// token by making another call. This can be used without a redirect URI, where the user inputs
    /// the code directly into the program.
    AuthorizationCode,

    /// Authorization directly returns an OAuth2 token. This can only be used with a redirect URI
    /// where the Dropbox server redirects the user's web browser to the program.
    ImplicitGrant,
}

impl Oauth2Type {
    pub fn as_str(self) -> &'static str {
        match self {
            Oauth2Type::AuthorizationCode => "code",
            Oauth2Type::ImplicitGrant => "token",
        }
    }
}

impl<'a> Oauth2AuthorizeUrlBuilder<'a> {
    pub fn new(client_id: &'a str, oauth2_type: Oauth2Type) -> Self {
        Self {
            client_id,
            response_type: oauth2_type.as_str(),
            force_reapprove: false,
            force_reauthentication: false,
            disable_signup: false,
            redirect_uri: None,
            state: None,
            require_role: None,
            locale: None,
        }
    }

    pub fn force_reapprove(mut self, value: bool) -> Self {
        self.force_reapprove = value;
        self
    }

    pub fn force_reauthentication(mut self, value: bool) -> Self {
        self.force_reauthentication = value;
        self
    }

    pub fn disable_signup(mut self, value: bool) -> Self {
        self.disable_signup = value;
        self
    }

    pub fn redirect_uri(mut self, value: &'a str) -> Self {
        self.redirect_uri = Some(value);
        self
    }

    pub fn state(mut self, value: &'a str) -> Self {
        self.state = Some(value);
        self
    }

    pub fn require_role(mut self, value: &'a str) -> Self {
        self.require_role = Some(value);
        self
    }

    pub fn locale(mut self, value: &'a str) -> Self {
        self.locale = Some(value);
        self
    }

    pub fn build(self) -> Url {
        let mut url = Url::parse("https://www.dropbox.com/oauth2/authorize").unwrap();
        {
            let mut params = url.query_pairs_mut();
            params.append_pair("response_type", self.response_type);
            params.append_pair("client_id", self.client_id);
            if self.force_reapprove {
                params.append_pair("force_reapprove", "true");
            }
            if self.force_reauthentication {
                params.append_pair("force_reauthentication", "true");
            }
            if self.disable_signup {
                params.append_pair("disable_signup", "true");
            }
            if let Some(value) = self.redirect_uri {
                params.append_pair("redirect_uri", value);
            }
            if let Some(value) = self.state {
                params.append_pair("state", value);
            }
            if let Some(value) = self.require_role {
                params.append_pair("require_role", value);
            }
            if let Some(value) = self.locale {
                params.append_pair("locale", value);
            }
        }
        url
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
