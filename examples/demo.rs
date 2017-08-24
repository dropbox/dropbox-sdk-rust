extern crate dropbox_sdk;
use dropbox_sdk::{files, HyperClient, Oauth2AuthorizeUrlBuilder, Oauth2Type};

extern crate env_logger;

use std::env;
use std::io::{self, Read, Write};

const CLIENT_ID: &'static str = "this is a fake client id";
const CLIENT_SECRET: &'static str = "this is a fake client secret";

fn main() {
    env_logger::init().unwrap();

    // Let the user pass the token in an environment variable, or prompt them if that's not found.
    let token = env::var("DBX_OAUTH_TOKEN").unwrap_or_else(|_| {
        let url = Oauth2AuthorizeUrlBuilder::new(CLIENT_ID, Oauth2Type::AuthorizationCode).build();
        eprintln!("Open this URL in your browser:");
        eprintln!("{}", url);
        eprintln!();
        eprintln!("Then paste the code here: ");

        let mut auth_code = String::new();
        io::stdin().read_line(&mut auth_code).unwrap();
        eprintln!();

        eprintln!("requesting OAuth2 token");
        match HyperClient::oauth2_token_from_authorization_code(
            CLIENT_ID, CLIENT_SECRET, auth_code.trim(), None)
        {
            Ok(token) => {
                eprintln!("got token");

                // This is where you'd save the token somewhere so you don't need to do this dance
                // again.

                token
            },
            Err(e) => {
                panic!("Error getting OAuth2 token: {}", e);
            }
        }
    });

    let client = HyperClient::new(token);
    let path = std::env::args().nth(1).expect("no filename given");

    eprintln!("downloading file {}", path);
    eprintln!();
    let result = files::download(&client, &files::DownloadArg::new(path), None, None);
    match result {
        Ok(Ok(download_result)) => {
            let mut body = download_result.body.expect("no body received!");
            let mut buf = [0u8; 4096];
            loop {
                match body.read(&mut buf) {
                    Ok(0) => { break; }
                    Ok(len) => {
                        io::stdout().write_all(&buf[0..len]).unwrap();
                    }
                    Err(e) => panic!("read error: {}", e)
                }
            }
        },
        Ok(Err(download_error)) => {
            eprintln!("Download error: {}", download_error);
        },
        Err(request_error) => {
            eprintln!("Failed to make the request: {}", request_error);
        }
    }
}
