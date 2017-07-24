//! Everything needed to implement your HTTP client.

use std::io::Read;

pub trait HttpClient {
    fn request(
        &self,
        endpoint: Endpoint,
        function: &str,
        params_json: String,
        body: Option<Vec<u8>>,
        range_start: Option<u64>,
        range_end: Option<u64>,
    ) -> ::Result<HttpRequestResultRaw>;
}

pub struct HttpRequestResultRaw {
    pub result_json: String,
    pub content_length: Option<u64>,
    pub body: Option<Box<Read>>,
}

pub enum Endpoint {
    Api,
    Content,
}

impl Endpoint {
    pub fn url(&self) -> &'static str {
        match *self {
            Endpoint::Api => "https://api.dropboxapi.com/2/",
            Endpoint::Content => "https://content.dropboxapi.com/2/",
        }
    }
}
