use std::io::Cursor;
use std::sync::Arc;
use dropbox_sdk::sync_routes::check;
use dropbox_sdk::client_trait::*;
use dropbox_sdk::client_trait_common::{HttpRequest, TeamSelect};
use dropbox_sdk::Error;

struct TestSyncClient;
struct TestRequest {
    url: String,
}

impl HttpClient for TestSyncClient {
    type Request = TestRequest;

    fn execute(&self, request: Self::Request, body: &[u8]) -> Result<HttpRequestResultRaw, Error> {
        match request.url.as_str() {
            "https://api.dropboxapi.com/2/check/user" => {
                let arg = serde_json::from_slice::<check::EchoArg>(body)?;
                Ok(HttpRequestResultRaw {
                    status: 200,
                    result_header: None,
                    content_length: None,
                    body: Box::new(Cursor::new(format!(r#"{{"result":"{}"}}"#, arg.query))),
                })
            }
            _ => Err(Error::HttpClient(Box::new(std::io::Error::other(format!("unhandled URL {}", request.url))))),
        }
    }

    fn new_request(&self, url: &str) -> Self::Request {
        TestRequest{ url: url.to_owned() }
    }

    fn update_token(&self, _old_token: Arc<String>) -> Result<bool, Error> {
        Ok(true)
    }

    fn token(&self) -> Option<Arc<String>> {
        Some(Arc::new(String::new()))
    }

    fn path_root(&self) -> Option<&str> {
        None
    }

    fn team_select(&self) -> Option<&TeamSelect> {
        None
    }
}

impl UserAuthClient for TestSyncClient {}

impl HttpRequest for TestRequest {
    fn set_header(self, _name: &str, _value: &str) -> Self {
        self
    }
}

#[test]
fn test_sync_client() {
    let client = TestSyncClient;
    let req = check::EchoArg::default().with_query("foobar".to_owned());
    let resp = check::user(&client, &req).expect("request must not fail");
    if resp.result != req.query {
        panic!("response mismatch");
    }
}
