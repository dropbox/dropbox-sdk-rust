// Copyright (c) 2019-2021 Dropbox, Inc.

use std::error::Error as StdError;
use crate::Error;
use crate::auth;
use crate::client_trait::*;
use serde::{Deserialize};
use serde::de::DeserializeOwned;
use serde::ser::Serialize;

/// When Dropbox returns an error with HTTP 409 or 429, it uses an implicit JSON object with the
/// following structure, which contains the actual error as a field.
#[derive(Debug, Deserialize)]
struct TopLevelError<T> {
    pub error: T,

    // It also has these fields, which we don't expose anywhere:
    //pub error_summary: String,
    //pub user_message: Option<String>,
}

/// This is mostly [`auth::RateLimitError`] but re-implemented here because it doesn't exactly match
/// the Stone type: `retry_after` is not actually specified in all responses, though it is marked as
/// a required field.
#[derive(Debug, Deserialize)]
struct RateLimitedError {
    pub reason: auth::RateLimitReason,

    #[serde(default)] // too_many_write_operations errors don't include this field; default to 0.
    pub retry_after: u32,
}

/// Does the request and returns a two-level result. The outer result has an error if something
/// went wrong in the process of making the request (I/O errors, parse errors, server 500 errors,
/// etc.). The inner result has an error if the server returned one for the request, otherwise it
/// has the deserialized JSON response and the body stream (if any).
#[allow(clippy::too_many_arguments)]
pub fn request_with_body<T: DeserializeOwned, E: DeserializeOwned + StdError, P: Serialize>(
    client: &impl HttpClient,
    endpoint: Endpoint,
    style: Style,
    function: &str,
    params: &P,
    body: Option<&[u8]>,
    range_start: Option<u64>,
    range_end: Option<u64>,
) -> crate::Result<Result<HttpRequestResult<T>, E>> {
    let params_json = serde_json::to_string(params)?;
    let result = client.request(endpoint, style, function, params_json, ParamsType::Json, body,
        range_start, range_end);
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
        Err(e) => {
            let innards = if let Error::UnexpectedHttpError {
                    ref code, ref status, ref json } = e {
                Some((*code, status.clone(), json.clone()))
            } else {
                None
            };

            // Try to turn the error into a more specific one.
            if let Some((code, status, response)) = innards {
                error!("HTTP {} {}: {}", code, status, response);
                return match code {
                    400 => {
                        Err(Error::BadRequest(response))
                    },
                    401 => {
                        match serde_json::from_str::<TopLevelError<auth::AuthError>>(&response) {
                            Ok(deserialized) => {
                                error!("auth error: {}", deserialized.error);
                                Err(Error::Authentication(deserialized.error))
                            }
                            Err(de_error) => {
                                error!("Failed to deserialize JSON from API error: {}", de_error);
                                Err(Error::Json(de_error))
                            }
                        }
                    },
                    403 => {
                        match serde_json::from_str::<TopLevelError<auth::AccessError>>(&response) {
                            Ok(deserialized) => {
                                error!("access denied: {:?}", deserialized.error);
                                Err(Error::AccessDenied(deserialized.error))
                            }
                            Err(de_error) => {
                                error!("Failed to deserialize JSON from API error: {}", de_error);
                                Err(Error::Json(de_error))
                            }
                        }
                    }
                    409 => {
                        // Response should be JSON-deseraializable into the strongly-typed
                        // error specified by type parameter E.
                        match serde_json::from_str::<TopLevelError<E>>(&response) {
                            Ok(deserialized) => {
                                error!("API error: {}", deserialized.error);
                                Ok(Err(deserialized.error))
                            },
                            Err(de_error) => {
                                error!("Failed to deserialize JSON from API error: {}", de_error);
                                Err(Error::Json(de_error))
                            }
                        }
                    },
                    429 => {
                        match serde_json::from_str::<TopLevelError<RateLimitedError>>(&response) {
                            Ok(deserialized) => {
                                let e = Error::RateLimited {
                                    reason: deserialized.error.reason,
                                    retry_after_seconds: deserialized.error.retry_after,
                                };
                                error!("{}", e);
                                Err(e)
                            }
                            Err(de_error) => {
                                error!("Failed to deserialize JSON from API error: {}", de_error);
                                Err(Error::Json(de_error))
                            }
                        }
                    },
                    500 ..= 599 => {
                        Err(Error::ServerError(response))
                    },
                    _ => {
                        Err(e)
                    }
                }
            } else if let Error::Json(ref json_err) = e {
                error!("JSON deserialization error: {}", json_err);
            } else {
                error!("HTTP request error: {}", e);
            }
            Err(e)
        }
    }
}

pub fn request<T: DeserializeOwned, E: DeserializeOwned + StdError, P: Serialize>(
    client: &impl HttpClient,
    endpoint: Endpoint,
    style: Style,
    function: &str,
    params: &P,
    body: Option<&[u8]>,
) -> crate::Result<Result<T, E>> {
    request_with_body(client, endpoint, style, function, params, body, None, None)
        .map(|result| result.map(|HttpRequestResult { result, .. }| result))
}
