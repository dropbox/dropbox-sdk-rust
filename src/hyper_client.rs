use ::ErrorKind;
use client_trait::{Endpoint, HttpClient, HttpRequestResultRaw};

pub struct HyperClient {
    // TODO
}

impl HttpClient for HyperClient {
    fn request(
        &self,
        endpoint: Endpoint,
        function: &str,
        params_json: String,
        body: Option<Vec<u8>>,
        range_start: Option<u64>,
        range_end: Option<u64>,
    ) -> ::Result<HttpRequestResultRaw> {
        unimplemented!()
    }
}