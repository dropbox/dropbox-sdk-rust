#![deny(rust_2018_idioms)]

//! This example illustrates a few basic Dropbox API operations: getting an OAuth2 token, listing
//! the contents of a folder recursively, and fetching a file given its path.

use tokio_util::compat::FuturesAsyncReadCompatExt;
use dropbox_sdk::default_async_client::{NoauthDefaultClient, UserAuthDefaultClient};
use dropbox_sdk::async_routes::files;

enum Operation {
    Usage,
    List(String),
    Download(String),
}

fn parse_args() -> Operation {
    let mut ctor: Option<fn(String) -> Operation> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" | "-h" => return Operation::Usage,
            "--list" => {
                ctor = Some(Operation::List);
            }
            "--download" => {
                ctor = Some(Operation::Download);
            }
            path if path.starts_with('/') => {
                return if let Some(ctor) = ctor {
                    ctor(arg)
                } else {
                    eprintln!("Either --download or --list must be specified");
                    Operation::Usage
                };
            }
            _ => {
                eprintln!("Unrecognized option {arg:?}");
                eprintln!();
                return Operation::Usage;
            }
        }
    }
    Operation::Usage
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let op = parse_args();

    if let Operation::Usage = op {
        eprintln!("usage: {} [option]", std::env::args().next().unwrap());
        eprintln!("    options:");
        eprintln!("        --help | -h          view this text");
        eprintln!("        --download <path>    copy the contents of <path> to stdout");
        eprintln!("        --list <path>        recursively list all files under <path>");
        eprintln!();
        eprintln!("    If a Dropbox OAuth token is given in the environment variable");
        eprintln!("    DBX_OAUTH_TOKEN, it will be used, otherwise you will be prompted for");
        eprintln!("    authentication interactively.");
        std::process::exit(1);
    }

    let mut auth = dropbox_sdk::oauth2::get_auth_from_env_or_prompt();
    if auth.save().is_none() {
        auth.obtain_access_token_async(NoauthDefaultClient::default()).await.unwrap();
        eprintln!("Next time set these environment variables to reuse this authorization:");
        eprintln!("  DBX_CLIENT_ID={}", auth.client_id());
        eprintln!("  DBX_OAUTH={}", auth.save().unwrap());
    }
    let client = UserAuthDefaultClient::new(auth);

    if let Operation::Download(path) = op {
        eprintln!("Copying file to stdout: {}", path);
        eprintln!();

        match files::download(&client, &files::DownloadArg::new(path), None, None).await {
            Ok(Ok(result)) => {
                match tokio::io::copy(
                    &mut result.body.expect("there must be a response body")
                        .compat(),
                    &mut tokio::io::stdout(),
                ).await {
                    Ok(n) => {
                        eprintln!("Downloaded {n} bytes");
                    }
                    Err(e) => {
                        eprintln!("I/O error: {e}");
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("Error from files/download: {e}");
            }
            Err(e) => {
                eprintln!("API request error: {e}");
            }
        }
    } else if let Operation::List(mut path) = op {
        eprintln!("Listing recursively: {path}");

        // Special case: the root folder is empty string. All other paths need to start with '/'.
        if path == "/" {
            path.clear();
        }

        let mut result = match files::list_folder(
            &client,
            &files::ListFolderArg::new(path).with_recursive(true),
        ).await {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                eprintln!("Error from files/list_folder: {e}");
                return;
            }
            Err(e) => {
                eprintln!("API request error: {e}");
                return;
            }
        };

        let mut num_entries = result.entries.len();
        let mut num_pages = 1;

        loop {
            for entry in result.entries {
                match entry {
                    files::Metadata::Folder(entry) => {
                        println!("Folder: {}", entry.path_display.unwrap_or(entry.name));
                    }
                    files::Metadata::File(entry) => {
                        println!("File: {}", entry.path_display.unwrap_or(entry.name));
                    }
                    files::Metadata::Deleted(entry) => {
                        panic!("unexpected deleted entry: {:?}", entry);
                    }
                }
            }

            if !result.has_more {
                break;
            }

            result = match files::list_folder_continue(
                &client,
                &files::ListFolderContinueArg::new(result.cursor),
            ).await {
                Ok(Ok(result)) => {
                    num_pages += 1;
                    num_entries += result.entries.len();
                    result
                }
                Ok(Err(e)) => {
                    eprintln!("Error from files/list_folder_continue: {e}");
                    break;
                }
                Err(e) => {
                    eprintln!("API request error: {e}");
                    break;
                }
            }
        }

        eprintln!("{num_entries} entries from {num_pages} result pages");
    }
}
