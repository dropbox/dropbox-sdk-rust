use bytes::Bytes;
use dropbox_sdk::async_client_trait::*;
use dropbox_sdk::async_routes::check;
use dropbox_sdk::Error;
use futures::io::Cursor;

struct TestAsyncClient;
struct TestRequest {
    url: String,
}

impl HttpClient for TestAsyncClient {
    type Request = TestRequest;

    async fn execute(
        &self,
        request: Self::Request,
        body: Bytes,
    ) -> Result<HttpRequestResultRaw, Error> {
        match request.url.as_str() {
            "https://api.dropboxapi.com/2/check/user" => {
                let arg = serde_json::from_slice::<check::EchoArg>(&body)?;

                // ensure the future isn't immediately ready
                tokio::task::yield_now().await;

                Ok(HttpRequestResultRaw {
                    status: 200,
                    result_header: None,
                    content_length: None,
                    body: Box::new(Cursor::new(
                        format!(r#"{{"result":"{}"}}"#, arg.query).into_bytes(),
                    )),
                })
            }
            _ => Err(Error::HttpClient(Box::new(std::io::Error::other(format!(
                "unhandled URL {}",
                request.url
            ))))),
        }
    }

    fn new_request(&self, url: &str) -> Self::Request {
        TestRequest {
            url: url.to_owned(),
        }
    }
}

impl UserAuthClient for TestAsyncClient {}

impl HttpRequest for TestRequest {
    fn set_header(self, _name: &str, _value: &str) -> Self {
        self
    }
}

#[tokio::test]
async fn test_sync_client() {
    let client = TestAsyncClient;
    let req = check::EchoArg::default().with_query("foobar".to_owned());
    let resp = check::user(&client, &req)
        .await
        .expect("request must not fail");
    if resp.result != req.query {
        panic!("response mismatch");
    }
}
