#![allow(unknown_lints)]

use std::io::{self, Read};
use std::str;

use ::ErrorKind;
use client_trait::{Endpoint, HttpClient, HttpRequestResultRaw};
use hyper::{self, Url};
use hyper::header::*;
use hyper_native_tls;

const USER_AGENT: &'static str = "Dropbox-APIv2-Rust/0.1";

pub struct HyperClient {
    client: hyper::client::Client,
    token: String,
}

impl HyperClient {
    pub fn new(token: String) -> HyperClient {
        let tls = hyper_native_tls::NativeTlsClient::new().unwrap();
        let https_connector = hyper::net::HttpsConnector::new(tls);
        let pool_connector = hyper::client::pool::Pool::with_connector(
            hyper::client::pool::Config { max_idle: 1 },
            https_connector);
        let client = hyper::client::Client::with_connector(pool_connector);
        HyperClient {
            client,
            token,
        }
    }
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
    ) -> super::Result<HttpRequestResultRaw> {

        let url = Url::parse(endpoint.url()).unwrap().join(function).expect("invalid request URL");
        debug!("request for {:?}", url);

        #[allow(never_loop)] // this is a false positive
        loop {
            let mut builder = self.client.post(url.clone());

            let mut headers = Headers::new();
            headers.set(UserAgent(USER_AGENT.to_owned()));
            headers.set(Authorization(Bearer { token: self.token.clone() }));
            headers.set(Connection::keep_alive());

            if let Some(start) = range_start {
                if let Some(end) = range_end {
                    headers.set(Range::Bytes(vec![ByteRangeSpec::FromTo(start, end)]));
                } else {
                    headers.set(Range::Bytes(vec![ByteRangeSpec::AllFrom(start)]));
                }
            } else if let Some(end) = range_end {
                headers.set(Range::Bytes(vec![ByteRangeSpec::Last(end)]));
            }

            // If the params are totally empt, don't send any arg header or body.
            if !params_json.is_empty() {
                match endpoint {
                    Endpoint::Api | Endpoint::Notify => {
                        // Send params in the body.
                        headers.set(ContentType::json());
                        builder = builder.body(params_json.as_bytes());
                        assert_eq!(None, body);
                    },
                    Endpoint::Content => {
                        // Send params in a header.
                        headers.set_raw("Dropbox-API-Arg", vec![params_json.clone().into_bytes()]);
                        if let Some(body) = body.as_ref() {
                            builder = builder.body(body.as_slice());
                        }
                    }
                }
            }

            let mut resp = match builder.headers(headers).send() {
                Ok(resp) => resp,
                Err(hyper::error::Error::Io(ref ioerr))
                        if ioerr.kind() == io::ErrorKind::ConnectionAborted => {
                    debug!("connection closed; retrying...");
                    continue;
                },
                Err(other) => {
                    error!("request failed: {}", other);
                    return Err(other.into());
                }
            };

            if !resp.status.is_success() {
                let (code, status) = {
                    let &hyper::http::RawStatus(ref code, ref status) = resp.status_raw();
                    use std::ops::Deref;
                    (*code, status.deref().to_owned())
                };
                let mut json = String::new();
                resp.read_to_string(&mut json)?;
                return Err(ErrorKind::ApiFailure(code, status, json).into());
            }

            return match endpoint {
                Endpoint::Api | Endpoint::Notify => {
                    // Get the response from the body; return no body stream.
                    let mut s = String::new();
                    resp.read_to_string(&mut s)?;
                    Ok(HttpRequestResultRaw {
                        result_json: s,
                        content_length: None,
                        body: None,
                    })
                },
                Endpoint::Content => {
                    // Get the response from a header; return the body stream.
                    let s = match resp.headers.get_raw("Dropbox-API-Result") {
                        Some(values) => {
                            String::from_utf8(values[0].clone())?
                        },
                        None => {
                            return Err(ErrorKind::ApiError("missing Dropbox-API-Result header").into());
                        }
                    };

                    let len = resp.headers.get::<ContentLength>().map(|h| h.0);

                    Ok(HttpRequestResultRaw {
                        result_json: s,
                        content_length: len,
                        body: Some(Box::new(resp)),
                    })
                }
            }

        }
    }
}
