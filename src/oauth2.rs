// Copyright (c) 2019-2020 Dropbox, Inc.

//! Helpers for requesting OAuth2 tokens.
//!
//! OAuth2 has a few possible ways to authenticate, and the right choice depends on how your app
//! operates and is deployed.
//!
//! For an overview, see the [Dropbox OAuth Guide].
//!
//! For quick recommendations based on the type of app you have, see the [OAuth types summary].
//!
//! [Dropbox OAuth Guide]: https://developers.dropbox.com/oauth-guide
//! [OAuth types summary]: https://developers.dropbox.com/oauth-guide#summary

use crate::Error;
use crate::client_trait::*;
use ring::rand::{SecureRandom, SystemRandom};
use url::form_urlencoded::Serializer as UrlEncoder;
use url::Url;

/// Which type of OAuth2 flow to use.
#[derive(Debug, Clone)]
pub enum Oauth2Type {
    /// The Authorization Code flow yields a temporary authorization code which must be turned into
    /// an OAuth2 token by making another call. The authorization page can do a web redirect back to
    /// your app with the code (if it is a server-side app), or can be used without a redirect URI,
    /// in which case the authorization page displays the authorization code to the user and they
    /// must then input the code manually into the program.
    AuthorizationCode(String /* client secret */),

    /// The PKCE flow is an extension of the Authorization Code flow which uses dynamically
    /// generated codes instead of an app secret to perform the OAuth exchange. This both avoids
    /// having a hardcoded secret in the app (useful for client-side / mobile apps) and also ensures
    /// that the authorization code can only be used by the client.
    PKCE(PkceCode),

    /// In Implicit Grant flow, the authorization page directly includes an OAuth2 token when it
    /// redirects the user's web browser back to your program, and no separate call to generate a
    /// token is needed. This can ONLY be used with a redirect URI.
    ///
    /// This flow is considered "legacy" and is not as secure as the other flows.
    ImplicitGrant(String /* client secret */),
}

impl Oauth2Type {
    /// The value to put in the "response_type" parameter to request the given token type.
    pub(crate) fn response_type_str(&self) -> &'static str {
        match self {
            Oauth2Type::AuthorizationCode { .. } | Oauth2Type::PKCE { .. } => "code",
            Oauth2Type::ImplicitGrant { .. } => "token",
        }
    }
}

/// What type of access token is requested? If unsure, ShortLivedAndRefresh is probably what you
/// want.
#[derive(Debug, Copy, Clone)]
pub enum TokenType {
    /// Return a short-lived bearer token and a long-lived refresh token that can be used to
    /// generate new bearer tokens in the future (as long as a user's approval remains valid).
    /// This is the default type for this SDK.
    ShortLivedAndRefresh,

    /// Return just the short-lived bearer token, without refresh token. The app will have to call
    /// [authorize] again to obtain a new token.
    ShortLived,

    /// Return a long-lived bearer token. The app must be allowed to do this in the Dropbox app
    /// console. This capability will be removed in the future.
    #[deprecated]
    LongLived,
}

impl TokenType {
    /// The value to put in the `token_access_type` parameter. If `None`, the parameter is omitted
    /// entirely.
    pub(crate) fn token_access_type_str(self) -> Option<&'static str> {
        match self {
            TokenType::ShortLivedAndRefresh => Some("offline"),
            TokenType::ShortLived => Some("online"),
            #[allow(deprecated)] TokenType::LongLived => None,
        }
    }
}

/// A proof key for OAuth2 PKCE ("Proof Key for Code Exchange") flow.
#[derive(Debug, Clone)]
pub struct PkceCode {
    code: String,
}

impl PkceCode {
    /// Generate a new random code string.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        // Spec lets us use [a-zA-Z0-9._~-] as the alphabet, and a length between 43 and 128.
        // A 93-byte input ends up as 125 base64 characters, so let's do that.
        let mut bytes = [0u8; 93];
        // not expecting this to ever actually fail:
        SystemRandom::new().fill(&mut bytes).expect("failed to get random bytes for PKCE");
        let code = base64::encode_config(bytes, base64::URL_SAFE);
        Self { code }
    }

    /// Get the SHA-256 hash as a base64-encoded string.
    pub fn s256(&self) -> String {
        let digest = ring::digest::digest(&ring::digest::SHA256, self.code.as_bytes());
        base64::encode_config(digest.as_ref(), base64::URL_SAFE_NO_PAD)
    }
}

/// Builds a URL that can be given to the user to visit to have Dropbox authorize your app.
///
/// If this app is a server-side app, you should redirect the user's browser to this URL to begin
/// the authorization, and set the redirect_uri to bring the user back to your site when done.
///
/// If this app is a client-side app, you should open a web browser with this URL to begin the
/// authorization, and set the redirect_uri to bring the user back to your app.
///
/// As a special case, if your app is a command-line application, you can skip setting the
/// redirect_uri and print this URL and instruct the user to open it in a browser. When they
/// complete the authorization, they will be given an auth code to input back into your app.
///
/// If you are using the deprecated Implicit Grant flow, the redirect after authentication will
/// provide you an OAuth2 token. In all other cases, you will have an authorization code, and you
/// must call [`oauth2_token_from_authorization_code`] to obtain a token.
#[derive(Debug)]
pub struct AuthorizeUrlBuilder<'a> {
    client_id: &'a str,
    flow_type: &'a Oauth2Type,
    token_type: TokenType,
    force_reapprove: bool,
    force_reauthentication: bool,
    disable_signup: bool,
    redirect_uri: Option<&'a str>,
    state: Option<&'a str>,
    require_role: Option<&'a str>,
    locale: Option<&'a str>,
    scope: Option<&'a str>,
}

impl<'a> AuthorizeUrlBuilder<'a> {
    /// Return a new builder for the given client ID and auth flow type, with all fields set to
    /// defaults.
    pub fn new(client_id: &'a str, flow_type: &'a Oauth2Type) -> Self {
        Self {
            client_id,
            flow_type,
            token_type: TokenType::ShortLivedAndRefresh,
            force_reapprove: false,
            force_reauthentication: false,
            disable_signup: false,
            redirect_uri: None,
            state: None,
            require_role: None,
            locale: None,
            scope: None,
        }
    }

    /// Set whether the user should be prompted to approve the request regardless of whether they
    /// have approved it before.
    pub fn force_reapprove(mut self, value: bool) -> Self {
        self.force_reapprove = value;
        self
    }

    /// Set whether the user should have to re-login when approving the request.
    pub fn force_reauthentication(mut self, value: bool) -> Self {
        self.force_reauthentication = value;
        self
    }

    /// Set whether new user signups should be allowed or not while approving the request.
    pub fn disable_signup(mut self, value: bool) -> Self {
        self.disable_signup = value;
        self
    }

    /// Set the URI the approve request should redirect the user to when completed.
    /// If no redirect URI is specified, the user will be shown the code directly and will have to
    /// manually input it into your app.
    pub fn redirect_uri(mut self, value: &'a str) -> Self {
        self.redirect_uri = Some(value);
        self
    }

    /// Up to 500 bytes of arbitrary data that will be passed back to your redirect URI. This
    /// parameter should be used to protect against cross-site request forgery (CSRF).
    pub fn state(mut self, value: &'a str) -> Self {
        self.state = Some(value);
        self
    }

    /// If this parameter is specified, the user will be asked to authorize with a particular type
    /// of Dropbox account, either `work` for a team account or `personal` for a personal account.
    /// Your app should still verify the type of Dropbox account after authorization since the user
    /// could modify or remove the require_role parameter.
    pub fn require_role(mut self, value: &'a str) -> Self {
        self.require_role = Some(value);
        self
    }

    /// Force a specific locale when prompting the user, instead of the locale indicated by their
    /// browser.
    pub fn locale(mut self, value: &'a str) -> Self {
        self.locale = Some(value);
        self
    }

    /// What type of token should be requested. Defaults to [`TokenType::ShortLivedAndRefresh`].
    pub fn token_type(mut self, value: TokenType) -> Self {
        self.token_type = value;
        self
    }

    /// This parameter allows your user to authorize a subset of the scopes selected in the
    /// App Console. Multiple scopes are separated by a space. If this parameter is omitted, the
    /// authorization page will request all scopes selected on the Permissions tab.
    pub fn scope(mut self, value: &'a str) -> Self {
        self.scope = Some(value);
        self
    }

    /// Build the OAuth2 authorization URL from the previously given parameters.
    pub fn build(self) -> Url {
        let mut url = Url::parse("https://www.dropbox.com/oauth2/authorize").unwrap();
        {
            let mut params = url.query_pairs_mut();
            params.append_pair("response_type", self.flow_type.response_type_str());
            params.append_pair("client_id", self.client_id);
            if let Some(val) = self.token_type.token_access_type_str() {
                params.append_pair("token_access_type", val);
            }
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
            if let Some(value) = self.scope {
                params.append_pair("scope", value);
            }
            if let Oauth2Type::PKCE(code) = self.flow_type {
                params.append_pair("code_challenge", &code.s256());
                params.append_pair("code_challenge_method", "S256");
            }
        }
        url
    }
}

/// [`Authorization`] is a state-machine.
///
/// Every flow starts with the `InitialAuth` state, which is just after the user authorizes the app
/// and gets redirected back. It then proceeds to either the `Refresh` or `AccessToken` state
/// depending on whether a long-lived token was requested.
///
/// `Refresh` contains the authorization code (from initial auth) and the refresh token necessary to
/// obtain updated short-lived access tokens.
///
/// `AccessToken` contains just the access token itself, which is either a long-lived access token
/// not expected to expire, or a short-lived token which, if it expires, cannot be refreshed except
/// by starting the authorization flow over again.
#[derive(Debug, Clone)]
enum AuthorizationState {
    InitialAuth {
        flow_type: Oauth2Type,
        auth_code: String,
        redirect_uri: Option<String>,
    },
    Refresh {
        auth_code: String,
        refresh_token: String,
    },
    AccessToken(String),
}

/// Provides for continuing authorization of the app.
#[derive(Debug, Clone)]
pub struct Authorization {
    client_id: String,
    state: AuthorizationState,
}

impl Authorization {
    /// Create a new instance using the authorization code provided by the authorization process
    /// upon redirect back to your app (or via manual user entry if not using a redirect URI).
    ///
    /// Requires the client ID; the type of OAuth2 flow being used (including the client secret or
    /// the PKCE challenge); the authorization code; and the redirect URI used for the original
    /// authorization request, if any.
    pub fn from_auth_code(
        client_id: String,
        flow_type: Oauth2Type,
        auth_code: String,
        redirect_uri: Option<String>,
    ) -> Self {
        Self {
            client_id,
            state: AuthorizationState::InitialAuth { flow_type, auth_code, redirect_uri },
        }
    }

    /// Recreate the authorization from a saved authorization code and refresh token.
    pub fn from_refresh_token(
        client_id: String,
        auth_code: String,
        refresh_token: String,
    ) -> Self {
        Self {
            client_id,
            state: AuthorizationState::Refresh { auth_code, refresh_token },
        }
    }

    /// Recreate the authorization from a saved long-lived access token. This token cannot be
    /// refreshed; any call to obtain_access_token will simply return the given token.
    pub fn from_access_token(
        client_id: String,
        access_token: String,
    ) -> Self {
        Self {
            client_id,
            state: AuthorizationState::AccessToken(access_token),
        }
    }

    /// Obtain an access token. Use this to complete the authorization process, or to obtain an
    /// updated token when a short-lived access token has expired.
    pub fn obtain_access_token(&mut self, client: impl NoauthClient) -> crate::Result<String> {
        let auth_code: String;
        let mut redirect_uri = None;
        let mut client_secret = None;
        let mut pkce_code = None;
        let mut refresh_token = None;

        match self.state.clone() {
            AuthorizationState::AccessToken(token) => {
                return Ok(token);
            }
            AuthorizationState::InitialAuth { flow_type, auth_code: code, redirect_uri: uri } => {
                match flow_type {
                    Oauth2Type::ImplicitGrant(_secret) => {
                        self.state = AuthorizationState::AccessToken(code.clone());
                        return Ok(code);
                    }
                    Oauth2Type::AuthorizationCode(secret) => {
                        client_secret = Some(secret);
                    }
                    Oauth2Type::PKCE(pkce) => {
                        pkce_code = Some(pkce.code);
                    }
                }
                auth_code = code;
                redirect_uri = uri;
            }
            AuthorizationState::Refresh { auth_code: code, refresh_token: refresh } => {
                auth_code = code;
                refresh_token = Some(refresh);
            }
        }

        let mut params = UrlEncoder::new(String::new());

        params.append_pair("code", &auth_code);

        if let Some(refresh) = refresh_token {
            params.append_pair("grant_type", "refresh_token");
            params.append_pair("refresh_token", &refresh);
        } else {
            params.append_pair("grant_type", "authorization_code");
        }

        params.append_pair("client_id", &self.client_id);

        if let Some(pkce) = pkce_code {
            params.append_pair("code_verifier", &pkce);
        } else {
            params.append_pair("client_secret", &client_secret.expect("need either PKCE code or client secret"));
        }

        if let Some(value) = redirect_uri {
            params.append_pair("redirect_uri", &value);
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

        let access_token: String;
        let refresh_token: Option<String>;

        match result_json {
            serde_json::Value::Object(mut map) => {
                match map.remove("access_token") {
                    Some(serde_json::Value::String(token)) => access_token = token,
                    _ => return Err(Error::UnexpectedResponse("no access token in response!")),
                }
                match map.remove("refresh_token") {
                    Some(serde_json::Value::String(refresh)) => refresh_token = Some(refresh),
                    Some(_) => return Err(Error::UnexpectedResponse("refresh token is not a string!")),
                    None => refresh_token = None,
                }
            },
            _ => return Err(Error::UnexpectedResponse("response is not a JSON object")),
        }

        match refresh_token {
            Some(refresh) => {
                self.state = AuthorizationState::Refresh { auth_code, refresh_token: refresh };
            }
            None => {
                self.state = AuthorizationState::AccessToken(access_token.clone());
            }
        }

        Ok(access_token)
    }
}