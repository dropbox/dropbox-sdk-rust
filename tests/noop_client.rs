use std::fmt::{Debug, Display, Formatter};
use dropbox_sdk::client_trait::*;

macro_rules! noop_client {
    ($name:ident) => {
        pub mod $name {
            use super::*;

            pub struct Client;

            impl HttpClient for Client {
                fn request(
                    &self,
                    _endpoint: Endpoint,
                    _style: Style,
                    function: &str,
                    _params: String,
                    _params_type: ParamsType,
                    _body: Option<&[u8]>,
                    _range_start: Option<u64>,
                    _range_end: Option<u64>,
                ) -> dropbox_sdk::Result<HttpRequestResultRaw> {
                    Err(dropbox_sdk::Error::HttpClient(Box::new(super::ErrMsg(format!("noop client called on {function}")))))
                }
            }
        }
    }
}

noop_client!(app);
noop_client!(noauth);
noop_client!(user);
noop_client!(team);

impl AppAuthClient for app::Client {}
impl NoauthClient for noauth::Client {}
impl UserAuthClient for user::Client {}
impl TeamAuthClient for team::Client {}

#[derive(Debug)]
struct ErrMsg(String);

impl Display for ErrMsg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ErrMsg{}
