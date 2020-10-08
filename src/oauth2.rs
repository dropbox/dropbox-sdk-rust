// Copyright (c) 2019-2020 Dropbox, Inc.

use crate::Error;
use crate::client_trait::*;
use url::form_urlencoded::Serializer as UrlEncoder;
use url::Url;

/// Given an authorization code, request an OAuth2 token from Dropbox API.
/// Requires the App ID and secret, as well as the redirect URI used in the prior authorize
/// request, if there was one.
pub fn oauth2_token_from_authorization_code(
    client: impl HttpClient,
    client_id: &str,
    client_secret: &str,
    authorization_code: &str,
    redirect_uri: Option<&str>,
) -> crate::Result<String> {

    let mut params = UrlEncoder::new(String::new());
    params.append_pair("code", authorization_code);
    params.append_pair("grant_type", "authorization_code");
    params.append_pair("client_id", client_id);
    params.append_pair("client_secret", client_secret);
    if let Some(value) = redirect_uri {
        params.append_pair("redirect_uri", value);
    }

    debug!("Requesting OAuth2 token");
    let resp = client.request(
        Endpoint::OAuth2,
        Style::Rpc,
        "oauth2/token",
        params.finish(),
        ParamsType::Form,
        None,
        None,
        None,
    )?;

    let result_json = serde_json::from_str(&resp.result_json)?;

    debug!("OAuth2 response: {:?}", result_json);
    match result_json {
        serde_json::Value::Object(mut map) => {
            match map.remove("access_token") {
                Some(serde_json::Value::String(token)) => Ok(token),
                _ => Err(Error::UnexpectedResponse("no access token in response!")),
            }
        },
        _ => Err(Error::UnexpectedResponse("response is not a JSON object")),
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
    /// token by making another call. This can be used without a redirect URI, where the user
    /// inputs the code directly into the program.
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
