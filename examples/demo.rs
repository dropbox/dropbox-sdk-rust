extern crate dropbox_sdk;
use dropbox_sdk::{files, HyperClient, Oauth2AuthorizeUrlBuilder, Oauth2Type};
use dropbox_sdk::client_trait::HttpClient;

extern crate env_logger;

use std::collections::VecDeque;
use std::env;
use std::io::{self, Read, Write};

const CLIENT_ID: &str = "this is a fake client id";
const CLIENT_SECRET: &str = "this is a fake client secret";

fn main() {
    env_logger::init();

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

    if let Some(path) = std::env::args().nth(1) {
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
    } else {
        eprintln!("listing all files");
        match list_directory(&client, "/", true) {
            Ok(Ok(iterator)) => {
                for entry_result in iterator {
                    match entry_result {
                        Ok(Ok(files::Metadata::Folder(entry))) => {
                            println!("Folder: {}", entry.path_display.unwrap_or(entry.name));
                        },
                        Ok(Ok(files::Metadata::File(entry))) => {
                            println!("File: {}", entry.path_display.unwrap_or(entry.name));
                        },
                        Ok(Ok(files::Metadata::Deleted(entry))) => {
                            panic!("unexpected deleted entry: {:?}", entry);
                        },
                        Ok(Err(e)) => {
                            eprintln!("Error from files/list_folder_continue: {}", e);
                            break;
                        },
                        Err(e) => {
                            eprintln!("API request error: {}", e);
                            break;
                        },
                    }
                }
            },
            Ok(Err(e)) => {
                eprintln!("Error from files/list_folder: {}", e);
            },
            Err(e) => {
                eprintln!("API request error: {}", e);
            }
        }
    }
}

fn list_directory<'a>(client: &'a HttpClient, path: &str, recursive: bool)
    -> dropbox_sdk::Result<Result<DirectoryIterator<'a>, files::ListFolderError>>
{
    assert!(path.starts_with('/'), "path needs to be absolute (start with a '/')");
    match files::list_folder(
        client,
        &files::ListFolderArg::new((&path[1..]).to_owned())
            .with_recursive(recursive))
    {
        Ok(Ok(result)) => {
            Ok(Ok(DirectoryIterator {
                client,
                buffer: result.entries.into(),
                cursor: Some(result.cursor),
            }))
        },
        Ok(Err(e)) => Ok(Err(e)),
        Err(e) => Err(e),
    }
}

struct DirectoryIterator<'a> {
    client: &'a HttpClient,
    buffer: VecDeque<files::Metadata>,
    cursor: Option<String>,
}

impl<'a> Iterator for DirectoryIterator<'a> {
    type Item = dropbox_sdk::Result<Result<files::Metadata, files::ListFolderContinueError>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(entry) = self.buffer.pop_front() {
            Some(Ok(Ok(entry)))
        } else if let Some(cursor) = self.cursor.take() {
            match files::list_folder_continue(self.client, &files::ListFolderContinueArg::new(cursor)) {
                Ok(Ok(result)) => {
                    self.buffer.extend(result.entries.into_iter());
                    if result.has_more {
                        self.cursor = Some(result.cursor);
                    }
                    self.buffer.pop_front().map(|entry| Ok(Ok(entry)))
                },
                Ok(Err(e)) => Some(Ok(Err(e))),
                Err(e) => Some(Err(e)),
            }
        } else {
            None
        }
    }
}
