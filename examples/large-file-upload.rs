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
use std::io::{self, Write, Seek, SeekFrom};
use std::process::exit;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

/// How many blocks to upload in parallel.
const PARALLELISM: usize = 20;

/// The size of a block. This is a Dropbox constant, not adjustable.
const BLOCK_SIZE: usize = 4 * 1024 * 1024;

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
    client: Arc<UserAuthDefaultClient>,
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
        let sesid = match files::upload_session_start(
            client.as_ref(),
            &files::UploadSessionStartArg::default()
                .with_session_type(files::UploadSessionType::Concurrent),
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

    let overall_start = Instant::now();
    let bytes_sofar = Arc::new(AtomicU64::new(0));

    {
        let client = client.clone();
        let session_id = Arc::new(cursor.session_id.clone());
        let start_offset = cursor.offset;
        if let Err(e) = parallel_reader::read_stream_and_process_chunks_in_parallel(
            &mut source_file,
            BLOCK_SIZE,
            PARALLELISM,
            Arc::new(move |block_offset, data: &[u8]| -> Result<(), String> {
                let cursor = files::UploadSessionCursor::new(
                    (*session_id).clone(),
                    start_offset + block_offset);
                let mut append_arg = files::UploadSessionAppendArg::new(cursor);
                if data.len() != BLOCK_SIZE {
                    // This must be the last block. Only the last one is allowed to be not 4 MiB
                    // exactly, so let's close the session.
                    append_arg.close = true;
                }
                upload_chunk_with_retry(
                    client.as_ref(),
                    &append_arg,
                    data,
                    overall_start,
                    bytes_sofar.as_ref(),
                    source_len - start_offset,
                    PARALLELISM as u64,
                )
            }))
        {
            return Err(e.to_string());
        }
    }

    eprintln!("committing...");
    let finish = files::UploadSessionFinishArg::new(
        cursor,
        files::CommitInfo::new(dest_path)
            .with_client_modified(iso8601(source_mtime)));

    let mut retry = 0;
    while retry < 3 {
        match files::upload_session_finish(client.as_ref(), &finish, &[]) {
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

fn upload_chunk_with_retry(
    client: &UserAuthDefaultClient,
    arg: &files::UploadSessionAppendArg,
    buf: &[u8],
    overall_start: Instant,
    bytes_sofar: &AtomicU64,
    total_bytes: u64,
    parallelism: u64,
) -> Result<(), String> {
    let chunk_start = Instant::now();
    let mut errors = 0;
    loop {
        match files::upload_session_append_v2(client, arg, buf) {
            Ok(Ok(())) => {
                break;
            }
            error => {
                errors += 1;
                let msg = format!("Error calling upload_session_append: {:?}", error);
                if errors == 3 {
                    return Err(msg);
                } else {
                    eprintln!("{}; retrying...", msg);
                }
            }
        }
    }

    let now = Instant::now();
    let chunk_time = now.duration_since(chunk_start);
    let overall_time = now.duration_since(overall_start);

    let chunk_bytes = buf.len() as u64;
    let bytes_sofar = bytes_sofar.fetch_add(chunk_bytes, SeqCst) + chunk_bytes;

    eprintln!("{:.01}%: {}Bytes uploaded, {}Bytes per second, {}Bytes per second average",
        bytes_sofar as f64 / total_bytes as f64 * 100.,
        human_number(bytes_sofar),
        human_number((chunk_bytes as f64 / chunk_time.as_secs_f64() * parallelism as f64) as u64),
        human_number((bytes_sofar as f64 / overall_time.as_secs_f64()) as u64),
        );

    Ok(())
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

    let client = Arc::new(UserAuthDefaultClient::new(get_oauth2_token()));

    let dest_path = get_destination_path(client.as_ref(), &args.dest_path, &args.source_path)
        .unwrap_or_else(|e| {
            fatal!("Error: {}", e);
        });

    eprintln!("source = {:?}", args.source_path);
    eprintln!("dest   = {:?}", dest_path);

    upload_file(client, source_file, dest_path, args.resume)
        .unwrap_or_else(|e| {
            fatal!("{}", e);
        });
}
