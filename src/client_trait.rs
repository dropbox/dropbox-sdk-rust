// Copyright (c) 2019 Dropbox, Inc.

//! Everything needed to implement your HTTP client.

use std::io::Read;

pub trait HttpClient {
    #[allow(clippy::too_many_arguments)]
    fn request(
        &self,
        endpoint: Endpoint,
        style: Style,
        auth: Auth,
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
    pub body: Option<Box<dyn Read>>,
}

pub struct HttpRequestResult<T> {
    pub result: T,
    pub content_length: Option<u64>,
    pub body: Option<Box<dyn Read>>,
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

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Auth {
    /// No authentication needed.
    Noauth,

    /// Either User or Team. Send a 'Authorization: Bearer <TOKEN>' header.
    Token,

    // TODO: not supported yet.
    // At least one route exists that can be used with both user and app auth, so we'd need some
    // way to let callers select between the two. See `files/get_thumbnail:2`.
    /*
    /// App authorization, Send a 'Authorization: Basic <base64(KEY:SECRET)>' header.
    App,
    */
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
