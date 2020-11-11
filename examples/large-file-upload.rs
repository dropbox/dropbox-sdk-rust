#![deny(rust_2018_idioms)]

//! This example illustrates advanced usage of Dropbox's chunked file upload API to upload large
//! files that would not fit in a single HTTP request, including allowing the user to resume
//! interrupted uploads.

use dropbox_sdk::files;
use dropbox_sdk::default_client::{NoauthDefaultClient, UserAuthDefaultClient};
use dropbox_sdk::oauth2::{oauth2_token_from_authorization_code, Oauth2AuthorizeUrlBuilder,
    Oauth2Type};

use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::process::exit;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

macro_rules! fatal {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
        exit(2);
    }
}

fn prompt(msg: &str) -> String {
    eprint!("{}: ", msg);
    io::stderr().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_owned()
}

fn human_number(n: u64) -> String {
    let mut f = n as f64;
    let prefixes = ['k','M','G','T','E'];
    let mut mag = 0;
    while mag < prefixes.len() {
        if f < 1000. {
            break;
        }
        f /= 1000.;
        mag += 1;
    }
    if mag == 0 {
        format!("{} ", n)
    } else {
        format!("{:.02} {}", f, prefixes[mag - 1])
    }
}

fn iso8601(t: SystemTime) -> String {
    let timestamp: i64 = match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };

    chrono::NaiveDateTime::from_timestamp(timestamp, 0 /* nsecs */)
        .format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

enum Operation {
    Usage,
    Upload(Args),
}

#[derive(Debug)]
struct Args {
    source_path: PathBuf,
    dest_path: String,
    resume: Option<Resume>,
}

#[derive(Debug)]
struct Resume {
    start_offset: u64,
    session_id: String,
}

fn parse_args() -> Operation {
    let mut a = std::env::args().skip(1);
    match (a.next(), a.next()) {
        (Some(ref arg), _) if arg == "--help" || arg == "-h" => {
            Operation::Usage
        }
        (Some(src), Some(dest)) => {
            let resume = match (a.next(), a.next()) {
                (Some(start_offset_str), Some(session_id)) => {
                    match start_offset_str.parse::<u64>() {
                        Ok(start_offset) => Some(Resume { start_offset, session_id }),
                        Err(e) => {
                            eprintln!("Invalid start offset: {}", e);
                            eprintln!("Usage: <source> <dest> <start offset> <session ID>");
                            None
                        }
                    }
                }
                (Some(_), None) => {
                    eprintln!("Usage: <source> <dest> <start offset> <session ID>");
                    None
                }
                _ => None,
            };
            Operation::Upload(Args {
                source_path: PathBuf::from(src),
                dest_path: dest,
                resume,
            })
        }
        (Some(_), None) => {
            eprintln!("missing destination path");
            Operation::Usage
        }
        (None, _) => {
            Operation::Usage
        }
    }
}

/// Similar to Read::read_exact except that this will partially fill the buffer on EOF instead of
/// returning an error.
/// The main reason this is needed is for reading from a stdin pipe, where normal Read::read may
/// stop after it reads only a few kbytes, but where we really want a much larger buffer to upload.
fn large_read(source: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut nread = 0;
    loop {
        match source.read(&mut buffer[nread ..]) {
            Ok(0) => {
                return Ok(nread);
            }
            Ok(n) => {
                nread += n;
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => (),
            Err(e) => {
                return Err(e);
            }
        }
    }
}

fn get_oauth2_token() -> String {
    std::env::var("DBX_OAUTH_TOKEN").unwrap_or_else(|_| {
        let client_id = prompt("Give me a Dropbox API app key");
        let client_secret = prompt("Give me a Dropbox API app secret");

        let url = Oauth2AuthorizeUrlBuilder::new(&client_id, Oauth2Type::AuthorizationCode).build();
        eprintln!("Open this URL in your browser:");
        eprintln!("{}", url);
        eprintln!();
        let auth_code = prompt("Then paste the code here");

        eprintln!("requesting OAuth2 token");
        match oauth2_token_from_authorization_code(
            NoauthDefaultClient::default(), &client_id, &client_secret, auth_code.trim(), None)
        {
            Ok(token) => {
                eprintln!("got token: {}", token);
                token
            }
            Err(e) => {
                fatal!("Error getting OAuth2 token: {}", e);
            }
        }
    })
}

/// Figure out if destination is a folder or not and change the destination path accordingly.
fn get_destination_path(client: &UserAuthDefaultClient, given_path: &str, source_path: &Path)
    -> Result<String, String>
{
    let filename = source_path.file_name()
        .ok_or_else(|| format!("invalid source path {:?} has no filename", source_path))?
        .to_string_lossy();

    // Special-case: we can't get metadata for the root, so just use the source path filename.
    if given_path == "/" {
        let mut path = "/".to_owned();
        path.push_str(&filename);
        return Ok(path);
    }

    let meta_result = files::get_metadata(
        client, &files::GetMetadataArg::new(given_path.to_owned()))
        .map_err(|e| format!("Request error while looking up destination: {}", e))?;

    match meta_result {
        Ok(files::Metadata::File(_)) => {
            // We're not going to allow overwriting existing files.
            Err(format!("Path {} already exists in Dropbox", given_path))
        }
        Ok(files::Metadata::Folder(_)) => {
            // Given destination path points to a folder, so append the source path's filename and
            // use that as the actual destination.

            let mut path = given_path.to_owned();
            path.push('/');
            path.push_str(&filename);

            Ok(path)
        }
        Ok(files::Metadata::Deleted(_)) => panic!("unexpected deleted metadata received"),
        Err(files::GetMetadataError::Path(files::LookupError::NotFound)) => {
            // Given destination path doesn't exist, which is just fine. Use the given path as-is.
            // Note that it's fine if the path's parents don't exist either; folders will be
            // automatically created as needed.
            Ok(given_path.to_owned())
        }
        Err(e) => Err(format!("Error looking up destination: {}", e))
    }
}

fn upload_file(
    client: &UserAuthDefaultClient,
    mut source_file: File,
    dest_path: String,
    resume: Option<Resume>,
) -> Result<(), String> {

    let (source_mtime, source_len) = source_file.metadata()
        .and_then(|meta| meta.modified().map(|mtime| (mtime, meta.len())))
        .map_err(|e| {
            format!("Error getting source file metadata: {}", e)
        })?;

    let cursor = if let Some(resume) = resume {
        eprintln!("Resuming upload: {:?}", resume);
        source_file.seek(SeekFrom::Start(resume.start_offset))
            .map_err(|e| format!("Seek error: {}", e))?;
        files::UploadSessionCursor::new(resume.session_id, resume.start_offset)
    } else {
        // TODO(wfraser) upload chunks in parallel
        let sesid = match files::upload_session_start(
            client,
            &files::UploadSessionStartArg::default(),
            &[])
        {
            Ok(Ok(result)) => result.session_id,
            error => {
                return Err(format!("Starting upload session failed: {:?}", error));
            }
        };

        files::UploadSessionCursor::new(sesid, 0)
    };

    eprintln!("upload session ID is {}", cursor.session_id);

    // Let's upload in 4 MiB chunks.
    let mut buf = vec![0; 4 * 1024 * 1024];

    let cursor = loop_with_progress(
        cursor,
        source_len,
        move |append_arg| upload_chunk(client, &mut source_file, append_arg, &mut buf))?;

    eprintln!("committing...");
    let finish = files::UploadSessionFinishArg::new(
        cursor,
        files::CommitInfo::new(dest_path)
            .with_client_modified(iso8601(source_mtime)));

    let mut retry = 0;
    while retry < 3 {
        match files::upload_session_finish(client, &finish, &[]) {
            Ok(Ok(file_metadata)) => {
                println!("Upload succeeded!");
                println!("{:#?}", file_metadata);
                return Ok(());
            }
            error => {
                eprintln!("Error finishing upload: {:?}", error);
                retry += 1;
                sleep(Duration::from_secs(1));
            }
        }
    }

    Err("Upload failed.".to_owned())
}

fn upload_chunk(
    client: &UserAuthDefaultClient,
    file: &mut impl Read,
    append_arg: &mut files::UploadSessionAppendArg,
    buf: &mut [u8],
) -> Result<u64, String> {

    let nread = large_read(file, buf)
        .map_err(|e| format!("Read error: {}", e))?;

    if nread == 0 {
        append_arg.close = true;
    }

    match files::upload_session_append_v2(client, append_arg, &buf[0..nread]) {
        Ok(Ok(())) => Ok(nread as u64),
        error => Err(format!("error calling upload_session_append: {:?}", error)),
    }
}

fn loop_with_progress(
    cursor: files::UploadSessionCursor,
    total_bytes: u64,
    mut f: impl FnMut(&mut files::UploadSessionAppendArg) -> Result<u64, String>,
) -> Result<files::UploadSessionCursor, String> {

    let mut append_arg = files::UploadSessionAppendArg::new(cursor);

    let start_time = Instant::now();
    let mut iter_start = start_time;
    let mut consecutive_errors = 0;
    while consecutive_errors < 3 {
        let num_bytes = match f(&mut append_arg) {
            Ok(n) => {
                consecutive_errors = 0;
                n
            }
            Err(e) => {
                eprintln!("{}", e);
                consecutive_errors += 1;
                sleep(Duration::from_secs(1));
                continue;
            }
        };

        append_arg.cursor.offset += num_bytes;

        if append_arg.close {
            return Ok(append_arg.cursor);
        }

        let now = Instant::now();
        let iter_time = now.duration_since(iter_start);
        let total_time = now.duration_since(start_time);
        iter_start = now;

        eprintln!("{:.01}%: {}Bytes uploaded, {}Bytes per second, {}Bytes per second average",
            append_arg.cursor.offset as f64 / total_bytes as f64 * 100.,
            human_number(append_arg.cursor.offset),
            human_number((num_bytes as f64 / iter_time.as_secs_f64()) as u64),
            human_number((append_arg.cursor.offset as f64 / total_time.as_secs_f64()) as u64));
    }

    Err(format!("Too many consecutive errors.\n\
        {} bytes uploaded before failure.\n\
        Session ID is {} if you wish to attempt to resume.",
        append_arg.cursor.offset, append_arg.cursor.session_id))
}

fn main() {
    env_logger::init();

    let args = match parse_args() {
        Operation::Usage => {
            fatal!("usage: {} <source> <Dropbox destination> [<resume offset> <resume session ID>]",
                      std::env::args().next().unwrap());
        }
        Operation::Upload(args) => args,
    };

    let source_file = File::open(&args.source_path)
        .unwrap_or_else(|e| {
            fatal!("Source file {:?} not found: {}", args.source_path, e);
        });

    let client = UserAuthDefaultClient::new(get_oauth2_token());

    let dest_path = get_destination_path(&client, &args.dest_path, &args.source_path)
        .unwrap_or_else(|e| {
            fatal!("Error: {}", e);
        });

    eprintln!("source = {:?}", args.source_path);
    eprintln!("dest   = {:?}", dest_path);

    upload_file(&client, source_file, dest_path, args.resume)
        .unwrap_or_else(|e| {
            fatal!("{}", e);
        });
}
