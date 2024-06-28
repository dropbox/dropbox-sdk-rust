#![deny(rust_2018_idioms)]

//! This example illustrates a few basic Dropbox API operations: getting an OAuth2 token, listing
//! the contents of a folder recursively, and fetching a file given its path.

use dropbox_sdk::default_client::{NoauthDefaultClient, UserAuthDefaultClient};
use dropbox_sdk::files;

use std::io;

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

fn main() {
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
        auth.obtain_access_token(NoauthDefaultClient::default()).unwrap();
        eprintln!("Next time set these environment variables to reuse this authorization:");
        eprintln!("  DBX_CLIENT_ID={}", auth.client_id());
        eprintln!("  DBX_OAUTH={}", auth.save().unwrap());
    }
    let client = UserAuthDefaultClient::new(auth);

    if let Operation::Download(path) = op {
        eprintln!("Copying file to stdout: {}", path);
        eprintln!();

        match files::download(&client, &files::DownloadArg::new(path), None, None) {
            Ok(result) => {
                match io::copy(
                    &mut result.body.expect("there must be a response body"),
                    &mut io::stdout(),
                ) {
                    Ok(n) => {
                        eprintln!("Downloaded {n} bytes");
                    }
                    Err(e) => {
                        eprintln!("I/O error: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("Error from files/download: {e}");
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
        ) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Error from files/list_folder: {e}");
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
            ) {
                Ok(result) => {
                    num_pages += 1;
                    num_entries += result.entries.len();
                    result
                }
                Err(e) => {
                    eprintln!("Error from files/list_folder_continue: {e}");
                    break;
                }
            }
        }

        eprintln!("{num_entries} entries from {num_pages} result pages");
    }
}
