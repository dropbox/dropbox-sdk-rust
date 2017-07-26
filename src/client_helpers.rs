use ResultExt;
use client_trait::*;
use serde::de::{Deserialize, DeserializeOwned, Deserializer};
use serde::ser::Serialize;
use serde_json;

#[derive(Debug)]
pub struct TopLevelError<T> {
    pub error_summary: String,
    pub user_message: Option<String>,
    pub error: T,
}

impl<'de, T> Deserialize<'de> for TopLevelError<T> {
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        unimplemented!()
    }
}

/// Does the request and returns a two-level result. The outer result has an error if something
/// went horribly wrong (I/O errors, parse errors, server 500 errors, etc.). The inner result has
/// an error if the server returned one for the request, otherwise it has the deserialized JSON
/// response and the body stream (if any).
pub fn request_with_body<T: DeserializeOwned, E: DeserializeOwned, P: Serialize>(
    client: &HttpClient,
    endpoint: Endpoint,
    function: &str,
    params: &P,
    body: Option<Vec<u8>>,
    range_start: Option<u64>,
    range_end: Option<u64>,
) -> super::Result<Result<HttpRequestResult<T>, E>> {
    let params_json = serde_json::to_string(params)?;
    let result = client.request(endpoint, function, params_json, body, range_start, range_end);
    match result {
        Ok(HttpRequestResultRaw { result_json, content_length, body }) => {
            debug!("json: {}", result_json);
            let result_value: T = serde_json::from_str(&result_json)?;
            Ok(Ok(HttpRequestResult {
                result: result_value,
                content_length,
                body,
            }))
        },
        Err(super::Error(super::ErrorKind::ApiFailure(ref code, ref _status, ref json), _)) if *code == 409 => {
            let err = serde_json::from_str::<TopLevelError<E>>(json)?;
            Ok(Err(err.error))
        },
        Err(e) => {
            error!("{}", e);
            Err(e).chain_err(|| super::ErrorKind::ApiError("API returned garbage"))
        }
    }
}

pub fn request<T: DeserializeOwned, E: DeserializeOwned, P: Serialize>(
    client: &HttpClient,
    endpoint: Endpoint,
    function: &str,
    params: &P,
    body: Option<Vec<u8>>,
) -> super::Result<Result<T, E>> {
    request_with_body(client, endpoint, function, params, body, None, None)
        .map(|result| result.map(|HttpRequestResult { result, .. }| result))
}
