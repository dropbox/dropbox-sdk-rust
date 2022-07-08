// Copyright (c) 2019-2021 Dropbox, Inc.

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
use std::env;
use std::io::{self, Write};
use std::sync::{Arc, RwLock};
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

    /// Return just the short-lived bearer token, without refresh token. The app will have to start
    /// the authorization flow again to obtain a new token.
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
/// must call make another call to obtain a token. See [`Authorization`], which is used to do this.
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
/// `Refresh` contains the refresh token necessary to obtain updated short-lived access tokens.
///
/// `AccessToken` contains just the access token itself, which is either a long-lived access token
/// not expected to expire, or a short-lived token which, if it expires, cannot be refreshed except
/// by starting the authorization flow over again.
#[derive(Debug, Clone)]
enum AuthorizationState {
    InitialAuth {
        client_id: String,
        flow_type: Oauth2Type,
        auth_code: String,
        redirect_uri: Option<String>,
    },
    Refresh {
        client_id: String,
        client_secret: String,
        refresh_token: String,
    },
    AccessToken(String),
}

/// Provides for continuing authorization of the app.
#[derive(Debug, Clone)]
pub struct Authorization {
    state: AuthorizationState,
}

impl Authorization {
    /// Create a new instance using the authorization code provided upon redirect back to your app
    /// (or via manual user entry if not using a redirect URI) after the user logs in.
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
            state: AuthorizationState::InitialAuth {
                client_id, flow_type, auth_code, redirect_uri },
        }
    }

    /// Save the authorization state to a string which can be reloaded later.
    ///
    /// Returns `None` if the state cannot be saved (e.g. authorization has not completed getting a
    /// token yet).
    pub fn save(&self) -> Option<String> {
        match &self.state {
            AuthorizationState::InitialAuth { .. } => None,
            AuthorizationState::AccessToken(access_token) =>
                Some(format!("1&{}", access_token)),
            AuthorizationState::Refresh { client_id: _, refresh_token, .. } =>
                Some(format!("2&{}", refresh_token)),
        }
    }

    /// Reload a saved authorization state produced by [`save`](Authorization::save).
    ///
    /// Returns `None` if the string could not be recognized. In this case, you should start the
    /// authorization procedure from scratch.
    ///
    /// Note that a loaded authorization state is not necessarily still valid and may produce
    /// [`Authentication`](crate::Error::Authentication) errors. In such a case you should also
    /// start the authorization procedure from scratch.
    pub fn load(client_id: String, client_secret: String, saved: &str) -> Option<Self> {
        let state = match saved.get(0..2) {
            Some("1&") => AuthorizationState::AccessToken(saved[2..].to_owned()),
            Some("2&") => AuthorizationState::Refresh {
                client_id,
                client_secret,
                refresh_token: saved[2..].to_owned(),
            },
            _ => {
                error!("unrecognized saved Authorization representation: {:?}", saved);
                return None;
            }
        };
        Some(Self { state })
    }

    /// Recreate the authorization from an authorization code and refresh token.
    pub fn from_refresh_token(
        client_id: String,
        client_secret: String,
        refresh_token: String,
    ) -> Self {
        Self { state: AuthorizationState::Refresh { client_id, client_secret, refresh_token } }
    }

    /// Recreate the authorization from a long-lived access token. This token cannot be refreshed;
    /// any call to [`obtain_access_token`](Authorization::obtain_access_token) will simply return
    /// the given token.
    pub fn from_access_token(
        access_token: String,
    ) -> Self {
        Self { state: AuthorizationState::AccessToken(access_token) }
    }

    /// Obtain an access token. Use this to complete the authorization process, or to obtain an
    /// updated token when a short-lived access token has expired.
    pub fn obtain_access_token(&mut self, client: impl NoauthClient) -> crate::Result<String> {
        let client_id: String;
        let mut redirect_uri = None;
        let mut client_secret = None;
        let mut pkce_code = None;
        let mut refresh_token = None;
        let mut auth_code = None;

        match self.state.clone() {
            AuthorizationState::AccessToken(token) => {
                return Ok(token);
            }
            AuthorizationState::InitialAuth {
                client_id: id, flow_type, auth_code: code, redirect_uri: uri } =>
            {
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
                client_id = id;
                auth_code = Some(code);
                redirect_uri = uri;
            }
            AuthorizationState::Refresh { client_id: id, client_secret: secret, refresh_token: refresh } => {
                client_id = id;
                client_secret = Some(secret);
                refresh_token = Some(refresh);
            }
        }

        let mut params = UrlEncoder::new(String::new());

        if let Some(refresh) = &refresh_token {
            params.append_pair("grant_type", "refresh_token");
            params.append_pair("refresh_token", refresh);
        } else {
            params.append_pair("grant_type", "authorization_code");
            params.append_pair("code", &auth_code.unwrap());
        }

        params.append_pair("client_id", &client_id);

        match pkce_code {
            Some(pkce) => {
                params.append_pair("code_verifier", &pkce);
            }
            None => {
                params.append_pair(
                    "client_secret",
                    client_secret.as_ref().expect("need either PKCE code or client secret"));
            }
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
                    Some(_) => {
                        return Err(Error::UnexpectedResponse("refresh token is not a string!"));
                    },
                    None => refresh_token = None,
                }
            },
            _ => return Err(Error::UnexpectedResponse("response is not a JSON object")),
        }

        match refresh_token {
            Some(refresh) => {
                // NOTE: currently breaks on PKCE flow
                self.state = AuthorizationState::Refresh { client_id, client_secret: client_secret.unwrap(), refresh_token: refresh };
            }
            None => {
                self.state = AuthorizationState::AccessToken(access_token.clone());
            }
        }

        Ok(access_token)
    }
}

/// `TokenCache` provides the current OAuth2 token and a means to refresh it in a thread-safe way.
pub struct TokenCache {
    auth: RwLock<(Authorization, Arc<String>)>,
}

impl TokenCache {
    /// Make a new token cache, using the given [`Authorization`] as a source of tokens.
    pub fn new(auth: Authorization) -> Self {
        Self {
            auth: RwLock::new((auth, Arc::new(String::new()))),
        }
    }

    /// Get the current token, or obtain one if no cached token is set yet.
    ///
    /// Unless the token has not been obtained yet, this does not do any HTTP request.
    pub fn get_token(&self, client: impl NoauthClient) -> crate::Result<Arc<String>> {
        let read = self.auth.read().unwrap();
        if read.1.is_empty() {
            let empty = Arc::clone(&read.1);
            drop(read);
            self.update_token(client, empty)
        } else {
            Ok(Arc::clone(&read.1))
        }
    }

    /// Forces an update to the token, for when it is detected that the token is expired.
    ///
    /// To avoid double-updating the token in a race, requires the token which is being replaced.
    pub fn update_token(&self, client: impl NoauthClient, old_token: Arc<String>)
        -> crate::Result<Arc<String>>
    {
        let mut write = self.auth.write().unwrap();
        // Check if the token changed while we were unlocked; only update it if it
        // didn't.
        if write.1 == old_token {
            write.1 = Arc::new(write.0.obtain_access_token(client)?);
        }
        Ok(Arc::clone(&write.1))
    }
}

/// Get an [`Authorization`] instance from environment variables `DBX_CLIENT_ID` and `DBX_OAUTH`
/// (containing a refresh token) or `DBX_OAUTH_TOKEN` (containing a legacy long-lived token).
///
/// If environment variables are not set, and stdin is a terminal, prompt interactively for
/// authorization.
///
/// If environment variables are not set, and stdin is not a terminal, panics.
///
/// This is a helper function intended only for tests and example code. Use in production code is
/// strongly discouraged; you should write something more customized to your needs instead.
///
/// In particular, in real production code, you probably don't want to use environment variables.
/// The client ID should be a hard-coded constant, or specified in configuration somewhere. It is
/// not something that will change often, or maybe ever.
/// The refresh token should only be stored somewhere safe like a file or database with restricted
/// access permissions.
pub fn get_auth_from_env_or_prompt() -> Authorization {
    if let Ok(long_lived) = env::var("DBX_OAUTH_TOKEN") {
        // Used to provide a legacy long-lived token.
        return Authorization::from_access_token(long_lived);
    }

    if let (Ok(client_id), Ok(client_secret), Ok(saved))
        = (env::var("DBX_CLIENT_ID"), env::var("DBX_CLIENT_SECRET"), env::var("DBX_OAUTH"))
        // important! see the above warning about using environment variables for this
    {
        match Authorization::load(client_id, client_secret, &saved) {
            Some(auth) => return auth,
            None => {
                eprintln!("saved authorization in DBX_CLIENT_ID and DBX_OAUTH are invalid");
                // and fall back to prompting
            }
        }
    }

    if !atty::is(atty::Stream::Stdin) {
        panic!("DBX_CLIENT_ID and/or DBX_OAUTH not set, and stdin not a TTY; cannot authorize");
    }

    fn prompt(msg: &str) -> String {
        eprint!("{}: ", msg);
        io::stderr().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_owned()
    }

    let client_id = prompt("Give me a Dropbox API app key");

    let oauth2_flow = Oauth2Type::PKCE(PkceCode::new());
    let url = AuthorizeUrlBuilder::new(&client_id, &oauth2_flow)
        .build();
    eprintln!("Open this URL in your browser:");
    eprintln!("{}", url);
    eprintln!();
    let auth_code = prompt("Then paste the code here");

    Authorization::from_auth_code(
        client_id,
        oauth2_flow,
        auth_code.trim().to_owned(),
        None,
    )
}
