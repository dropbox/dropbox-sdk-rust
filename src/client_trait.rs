//! Everything needed to implement your HTTP client.

use std::io::Read;

pub trait HttpClient {
    #[cfg_attr(feature="cargo-clippy", allow(too_many_arguments))]
    fn request(
        &self,
        endpoint: Endpoint,
        style: Style,
        function: &str,
        params_json: String,
        body: Option<&[u8]>,
        range_start: Option<u64>,
        range_end: Option<u64>,
    ) -> crate::Result<HttpRequestResultRaw>;
}

pub struct HttpRequestResultRaw {
    pub result_json: String,
    pub content_length: Option<u64>,
    pub body: Option<Box<Read>>,
}

pub struct HttpRequestResult<T> {
    pub result: T,
    pub content_length: Option<u64>,
    pub body: Option<Box<Read>>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Endpoint {
    Api,
    Content,
    Notify,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Style {
    Rpc,
    Upload,
    Download,
}

impl Endpoint {
    pub fn url(self) -> &'static str {
        match self {
            Endpoint::Api => "https://api.dropboxapi.com/2/",
            Endpoint::Content => "https://content.dropboxapi.com/2/",
            Endpoint::Notify => "https://notify.dropboxapi.com/2/",
        }
    }
}
