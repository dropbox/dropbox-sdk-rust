use std::fmt::{Debug, Display, Formatter};
use dropbox_sdk::client_trait::*;
use dropbox_sdk::client_trait_common::HttpRequest;

macro_rules! noop_client {
    ($name:ident) => {
        pub mod $name {
            use super::*;

            pub struct Client;

            impl HttpClient for Client {
                type Request = NoopRequest;

                fn execute(
                    &self,
                    _request: Self::Request,
                    _body: &[u8],
                ) -> dropbox_sdk::Result<HttpRequestResultRaw> {
                    Err(dropbox_sdk::Error::HttpClient(Box::new(super::ErrMsg("noop client called".to_owned()))))
                }

                fn new_request(&self, _url: &str) -> Self::Request {
                    NoopRequest {}
                }
            }
        }
    }
}

noop_client!(app);
noop_client!(noauth);
noop_client!(user);
noop_client!(team);

pub struct NoopRequest {}

impl HttpRequest for NoopRequest {
    fn set_header(self, _name: &str, _value: &str) -> Self {
        self
    }
}

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
