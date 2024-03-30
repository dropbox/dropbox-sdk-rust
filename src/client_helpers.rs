// Copyright (c) 2019-2021 Dropbox, Inc.

use std::error::Error as StdError;
use std::future::Future;
use std::io::ErrorKind;
use std::sync::Arc;
use bytes::Bytes;
use futures::{AsyncRead, AsyncReadExt, FutureExt};
use serde::{Deserialize};
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use crate::Error;
use crate::async_client_trait::{HttpClient, HttpRequestResult, HttpRequestResultRaw, SyncReadAdapter};
use crate::auth::{AccessError, AuthError, RateLimitReason};
use crate::client_trait as sync;
use crate::client_trait_common::{Endpoint, HttpRequest, ParamsType, Style, TeamSelect};

/// When Dropbox returns an error with HTTP 409 or 429, it uses an implicit JSON object with the
/// following structure, which contains the actual error as a field.
#[derive(Debug, Deserialize)]
pub(crate) struct TopLevelError<T> {
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
    pub reason: RateLimitReason,

    #[serde(default)] // too_many_write_operations errors don't include this field; default to 0.
    pub retry_after: u32,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_request<T: HttpClient>(
    client: &T,
    endpoint: Endpoint,
    style: Style,
    function: &str,
    params: String,
    params_type: ParamsType,
    body: Bytes,
    range_start: Option<u64>,
    range_end: Option<u64>,
    token: Option<&str>,
    path_root: Option<&str>,
    team_select: Option<&TeamSelect>,
) -> T::Request {
    let url = endpoint.url().to_owned() + function;

    let mut req = client.new_request(&url);
    req = req.set_header("User-Agent", concat!("Dropbox-SDK-Rust/", env!("CARGO_PKG_VERSION")));

    if let Some(token) = token {
        req = req.set_header("Authorization", &format!("Bearer {token}"));
    }

    if let Some(path_root) = path_root {
        req = req.set_header("Dropbox-API-Path-Root", path_root);
    }

    if let Some(team_select) = team_select {
        req = match team_select {
            TeamSelect::User(id) => req.set_header("Dropbox-API-Select-User", id),
            TeamSelect::Admin(id) => req.set_header("Dropbox-API-Select-Admin", id),
        };
    }

    req = match (range_start, range_end) {
        (Some(start), Some(end)) => req.set_header("Range", &format!("bytes={start}-{end}")),
        (Some(start), None) => req.set_header("Range", &format!("bytes={start}-")),
        (None, Some(end)) => req.set_header("Range", &format!("bytes=-{end}")),
        (None, None) => req,
    };

    if !params.is_empty() {
        match style {
            Style::Rpc => {
                // Send params in the body.
                req = req.set_header("Content-Type", params_type.content_type());
                req = req.set_body(Bytes::from(params));
            }
            Style::Upload => {
                // Send params in a header.
                req = req.set_header("Dropbox-API-Arg", &params);
                req = req.set_header("Content-Type", "application/octet-stream");
                req = req.set_body(body);
            }
            Style::Download => {
                // Send params in a header.
                req = req.set_header("Dropbox-API-Arg", &params);
                assert!(body.is_empty(), "body can only be set for Style::Upload request");
            }
        }
    }

    req
}

pub(crate) async fn body_to_string(body: &mut (dyn AsyncRead + Unpin)) -> crate::Result<String> {
    let mut s = String::new();
    match body.read_to_string(&mut s).await {
        Ok(_) => Ok(s),
        Err(e) => {
            if e.kind() == ErrorKind::InvalidData {
                Err(Error::UnexpectedResponse(format!("invalid response: {e}")))
            } else {
                Err(Error::HttpClient(Box::new(e)))
            }
        }
    }
}

/// Does the request and returns a two-level result. The outer result has an error if something
/// went wrong in the process of making the request (I/O errors, parse errors, server 500 errors,
/// etc.). The inner result has an error if the server returned one for the request, otherwise it
/// has the deserialized JSON response and the body stream (if any).
#[allow(clippy::too_many_arguments)]
pub async fn request_with_body<T, E, P, C>(
    client: &C,
    endpoint: Endpoint,
    style: Style,
    function: &str,
    params: &P,
    body: Option<&[u8]>,
    range_start: Option<u64>,
    range_end: Option<u64>,
) -> crate::Result<Result<HttpRequestResult<T>, E>> where
    T: DeserializeOwned,
    E: DeserializeOwned + StdError,
    P: Serialize,
    C: HttpClient,
{
    let body = if let Some(body) = body {
        Bytes::copy_from_slice(body) // FIXME: input arg should be Bytes instead
    } else {
        Bytes::new()
    };
    let mut retried = false;
    'auth_retry: loop {
        let params_json = serde_json::to_string(params)?;
        let token = client.token();
        if token.is_none()
            && !retried
            && client.update_token(Arc::new(String::new())).await
        {
                retried = true;
                continue 'auth_retry;
        }
        let req = prepare_request(
            client,
            endpoint,
            style,
            function,
            params_json,
            ParamsType::Json,
            body.clone(),
            range_start,
            range_end,
            token.as_ref().map(|t| t.as_str()),
            client.path_root(),
            client.team_select(),
        );
        let result = client.execute(req).await;
        return match result {
            Ok(HttpRequestResultRaw {
                   status: (code, status),
                   result_header,
                   content_length,
                   mut body
            }) => {
                let (response, body) = match style {
                    Style::Rpc | Style::Upload => {
                        // Read the response from the body.
                        if let Some(header) = result_header {
                            return Err(Error::UnexpectedResponse(format!("unexpected response in header, expected it in the body: {header}")));
                        } else {
                            (body_to_string(&mut body).await?, None)
                        }
                    }
                    Style::Download => {
                        // Get the response from the header.
                        if let Some(header) = result_header {
                            (header, Some(body))
                        } else {
                            return Err(Error::UnexpectedResponse("expected a Dropbox-API-Result header".to_owned()));
                        }
                    }
                };

                if (200..300).contains(&code) {
                    Ok(Ok(HttpRequestResult {
                        result: serde_json::from_str(&response)?,
                        content_length,
                        body,
                    }))
                } else {
                    error!("HTTP {code} {status}");
                    match code {
                        400 => {
                            Err(Error::BadRequest(response))
                        },
                        401 => {
                            match serde_json::from_str::<TopLevelError<AuthError>>(&response) {
                                Ok(deserialized) => {
                                    error!("auth error: {}", deserialized.error);
                                    if deserialized.error == AuthError::ExpiredAccessToken && !retried {
                                        let old_token = token.unwrap_or_else(|| Arc::new(String::new()));
                                        if client.update_token(old_token).await {
                                            retried = true;
                                            continue 'auth_retry;
                                        }
                                    }
                                    Err(Error::Authentication(deserialized.error))
                                }
                                Err(de_error) => {
                                    error!("Failed to deserialize JSON from API error: {}", de_error);
                                    Err(Error::Json(de_error))
                                }
                            }
                        },
                        403 => {
                            match serde_json::from_str::<TopLevelError<AccessError>>(&response) {
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
                        500..=599 => {
                            Err(Error::ServerError(response))
                        },
                        _ => {
                            Err(Error::UnexpectedHttpError {
                                code,
                                status,
                                json: response,
                            })
                        }
                    }
                }
            }
            Err(e) => Err(e),
        };
    }
}

pub async fn request<T: DeserializeOwned, E: DeserializeOwned + StdError, P: Serialize>(
    client: &impl HttpClient,
    endpoint: Endpoint,
    style: Style,
    function: &str,
    params: &P,
    body: Option<&[u8]>,
) -> crate::Result<Result<T, E>> {
    request_with_body(client, endpoint, style, function, params, body, None, None)
        .await
        .map(|result| result.map(|HttpRequestResult { result, .. }| result))
}

/// Given an async HttpRequestResult which was created from a *sync* HttpClient, convert it to the
/// sync HttpRequestResult by cracking open the SyncReadAdapter in the body.
///
/// This is ONLY safe if the result was created by a sync HttpClient, so we require it as an
/// argument just to be extra careful.
#[inline]
pub(crate) fn unwrap_async_result<T>(r: HttpRequestResult<T>, _client: &impl HttpClient) -> sync::HttpRequestResult<T> {
    match r.body {
        Some(async_read) => {
            let p: *mut dyn AsyncRead = Box::into_raw(async_read);
            // SAFETY: the only body value an async HttpRequestResult created for a sync client
            // can be is a SyncReadAdapter.
            let adapter = unsafe {
                Box::<SyncReadAdapter>::from_raw(p as *mut SyncReadAdapter)
            };
            sync::HttpRequestResult {
                result: r.result,
                content_length: r.content_length,
                body: Some(adapter.inner),
            }
        }
        None => sync::HttpRequestResult {
            result: r.result,
            content_length: r.content_length,
            body: None,
        }
    }
}

#[inline]
pub(crate) fn unwrap_async_body<T, E>(
    f: impl Future<Output = crate::Result<Result<HttpRequestResult<T>, E>>>,
    client: &impl HttpClient,
) -> crate::Result<Result<sync::HttpRequestResult<T>, E>> {
    let r = f.now_or_never().expect("sync future should resolve immediately");
    match r {
        Ok(Ok(v)) => Ok(Ok(unwrap_async_result(v, client))),
        Ok(Err(e)) => Ok(Err(e)),
        Err(e) => Err(e),
    }
}

#[inline]
pub(crate) fn unwrap_async<T, E>(
    f: impl Future<Output = crate::Result<Result<T, E>>>,
) -> crate::Result<Result<T, E>> {
    f.now_or_never().expect("sync future should resolve immediately")
}
