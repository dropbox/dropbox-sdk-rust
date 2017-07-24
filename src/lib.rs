#[macro_use] extern crate error_chain;
#[cfg(feature = "hyper_client")] extern crate hyper;
#[cfg(feature = "hyper_client")] extern crate hyper_native_tls;
#[macro_use] extern crate log;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    foreign_links {
        Hyper(hyper::Error) #[cfg(feature = "hyper_client")]; // TODO: this should be made more abstract
        Io(std::io::Error);
        Json(serde_json::Error);
        Utf8(std::string::FromUtf8Error);
    }

    errors {
        /// The API returned something invalid.
        ApiError(reason: &'static str) {
            description("Dropbox unexpected API error")
            display("Dropbox unexpected API error: {}", reason)
        }

        /// The API returned an error code.
        ApiFailure(code: u16, status: String, json: String) {
            description("Dropbox API returned failure")
            display("Dropbox API returned {} - {}", status, json)
        }
    }
}

#[cfg(feature = "hyper_client")] mod hyper_client;
#[cfg(feature = "hyper_client")] pub use hyper_client::HyperClient;

pub mod client_trait;
pub(crate) mod client_helpers;

mod generated; // You need to run the Stone generator to create this module.
pub use generated::*;
