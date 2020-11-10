#![deny(rust_2018_idioms)]

use dropbox_sdk::oauth2::{oauth2_token_from_authorization_code, Oauth2AuthorizeUrlBuilder,
    Oauth2Type};
use dropbox_sdk::files;
use dropbox_sdk::default_client::{NoauthDefaultClient, UserAuthDefaultClient};

use std::fs::File;
use std::path::PathBuf;
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::time::{Duration, Instant, SystemTime};

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

fn main() {
    env_logger::init();

    let mut args = match parse_args() {
        Operation::Usage => {
            eprintln!("usage: {} <source> <Dropbox destination> [<resume offset> <resume session ID>]",
                      std::env::args().next().unwrap());
            std::process::exit(1);
        }
        Operation::Upload(args) => args,
    };

    let mut source_file = File::open(&args.source_path)
        .unwrap_or_else(|e| {
            eprintln!("Source file {:?} not found: {}", args.source_path, e);
            std::process::exit(2);
        });
    let (source_mtime, source_len) = source_file.metadata()
        .and_then(|meta| meta.modified().map(|mtime| (mtime, meta.len())))
        .unwrap_or_else(|e| {
            eprintln!("Error getting source file {:?} metadata: {}", args.source_path, e);
            std::process::exit(2);
        });

    let token = std::env::var("DBX_OAUTH_TOKEN").unwrap_or_else(|_| {
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
                eprintln!("Error getting OAuth2 token: {}", e);
                std::process::exit(2);
            }
        }
    });

    let client = UserAuthDefaultClient::new(token);

    // Figure out if destination is a folder or not and change the destination path accordingly.
    let dest_path = match files::get_metadata(
        &client,
        &files::GetMetadataArg::new(args.dest_path.clone()))
    {
        Ok(Ok(files::Metadata::File(_meta))) => {
            eprintln!("Error: \"{}\" already exists in Dropbox", args.dest_path);
            std::process::exit(2);
        }
        Ok(Ok(files::Metadata::Folder(_meta))) => {
            eprintln!("Destination is a folder; appending filename.");
            let mut path = args.dest_path.split_off(0);
            path.push('/');
            path.push_str(
                &args.source_path.file_name()
                    .unwrap_or_else(|| {
                        eprintln!("Invalid source path {:?}", args.source_path);
                        std::process::exit(2);
                    })
                    .to_string_lossy());
            path

            // TODO: check for this file as well
        }
        Ok(Ok(files::Metadata::Deleted(_))) => {
            panic!("unexpected deleted metadata received");
        }
        Ok(Err(files::GetMetadataError::Path(files::LookupError::NotFound))) => {
            // File not found; totally okay.
            // TODO: make it not log to the console when this happens
            args.dest_path.split_off(0)
        }
        Ok(Err(files::GetMetadataError::Path(e))) => {
            eprintln!("Error looking up destination: {}", e);
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("Request error while looking up destination: {}", e);
            std::process::exit(2);
        }
    };

    eprintln!("source = {:?}", args.source_path);
    eprintln!("dest   = {:?}", dest_path);

    let session_id = match args.resume {
        Some(ref resume) => resume.session_id.clone(),
        None => {
            // TODO(wfraser) upload chunks in parallel
            match files::upload_session_start(
                &client, &files::UploadSessionStartArg::default(), &[])
            {
                Ok(Ok(result)) => result.session_id,
                error => {
                    eprintln!("Starting upload session failed: {:?}", error);
                    std::process::exit(2);
                }
            }
        }
    };

    eprintln!("upload session ID is {}", session_id);

    let mut append_arg = files::UploadSessionAppendArg::new(
        files::UploadSessionCursor::new(session_id.clone(), 0));

    // Upload this many bytes in each request. The smaller this is, the more HTTP request overhead
    // there will be. But the larger it is, the more bandwidth that is potentially wasted on
    // network errors.
    const BUF_SIZE: usize = 32 * 1024 * 1024;

    // if the buffer is small we can stack-allocate it:
    //let mut buf = [0u8; BUF_SIZE];
    // otherwise it has to be heap-allocated:
    let mut buf = vec![0; BUF_SIZE];

    let start_time = Instant::now();
    let mut last_time = Instant::now();
    let mut bytes_out = 0u64;
    let mut succeeded = false;

    if let Some(resume) = args.resume {
        eprintln!("Resuming upload: {:?}", resume);
        source_file.seek(SeekFrom::Start(resume.start_offset)).unwrap_or_else(|e| {
            eprintln!("Seek error: {}", e);
            std::process::exit(2);
        });
        bytes_out = resume.start_offset;
        append_arg.cursor.offset = resume.start_offset;
    }

    loop {
        let nread = large_read(&mut source_file, &mut buf)
            .unwrap_or_else(|e| {
                eprintln!("Read error: {}", e);
                std::process::exit(2);
            });
        if bytes_out < source_len && bytes_out + nread as u64 > source_len {
            eprintln!("WARNING: read past the initial end of the file");
            eprintln!("({} bytes vs {} expected)", bytes_out + nread as u64, source_len);
        }
        if nread == 0 {
            if bytes_out < source_len {
                eprintln!("WARNING: read short of the initial end of the file");
                eprintln!("({} bytes vs {} expected)", bytes_out, source_len);
            }
            break;
        }

        succeeded = false;
        let mut consecutive_errors = 0;
        while consecutive_errors < 3 {
            match files::upload_session_append_v2(&client, &append_arg, &buf[0..nread]) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("Error appending data: {}", e);
                    consecutive_errors += 1;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                Err(e) => {
                    eprintln!("Error appending data: {}", e);
                    consecutive_errors += 1;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }

            succeeded = true;
            break;
        }

        if !succeeded {
            break;
        }

        bytes_out += nread as u64;
        append_arg.cursor.offset += nread as u64;

        let now = Instant::now();
        let time = now.duration_since(last_time);
        let total_time = now.duration_since(start_time);
        let millis = time.as_secs() * 1000 + u64::from(time.subsec_millis());
        let total_millis = total_time.as_secs() * 1000 + u64::from(total_time.subsec_millis());
        last_time = now;

        eprintln!("{:.01}%: {}Bytes uploaded, {}Bytes per second, {}Bytes per second average",
                  bytes_out as f64 / source_len as f64 * 100.,
                  human_number(bytes_out),
                  human_number(nread as u64 * 1000 / millis),
                  human_number(bytes_out * 1000 / total_millis));
    }

    if !succeeded {
        println!("Upload failed!");
        println!("{} bytes uploaded before failure.", bytes_out);
        println!("Session ID is {} if you wish to attempt to resume.", session_id);
    } else {
        eprintln!("committing...");
        let finish = files::UploadSessionFinishArg::new(
            append_arg.cursor,
            files::CommitInfo::new(dest_path)
                .with_client_modified(iso8601(source_mtime)));

        let mut retry = 0;
        succeeded = false;
        while retry < 3 {
            match files::upload_session_finish(&client, &finish, &[]) {
                Ok(Ok(filemetadata)) => {
                    println!("Upload succeeded!");
                    println!("{:#?}", filemetadata);
                }
                Ok(Err(e)) => {
                    eprintln!("Error finishing upload: {}", e);
                    retry += 1;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                Err(e) => {
                    eprintln!("Error finishing upload: {}", e);
                    retry += 1;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
            succeeded = true;
            break;
        }
    }

    if !succeeded {
        std::process::exit(2);
    }
}
