extern crate dropbox_sdk;
use dropbox_sdk::{HyperClient, Oauth2AuthorizeUrlBuilder, Oauth2Type};
use dropbox_sdk::files;

extern crate chrono;
extern crate env_logger;

use std::fs::File;
use std::path::PathBuf;
use std::io::{self, Read, Write};
use std::time::{Instant, SystemTime};

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
        Err(e) => e.duration().as_secs() as i64 * -1,
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
}

fn parse_args() -> Operation {
    let mut a = std::env::args().skip(1);
    match (a.next(), a.next()) {
        (Some(ref arg), _) if arg == "--help" || arg == "-h" => {
            Operation::Usage
        }
        (Some(src), Some(dest)) => {
            Operation::Upload(Args {
                source_path: PathBuf::from(src),
                dest_path: dest,
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

fn main() {
    env_logger::init();

    let mut args = match parse_args() {
        Operation::Usage => {
            eprintln!("usage: {} <source> <Dropbox destination>", std::env::args().nth(0).unwrap());
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
        match HyperClient::oauth2_token_from_authorization_code(
            &client_id, &client_secret, auth_code.trim(), None)
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

    let client = HyperClient::new(token);

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

    let session_id = match files::upload_session_start(
        &client, &files::UploadSessionStartArg::default(), &[])
    {
        Ok(Ok(result)) => result.session_id,
        Ok(Err(())) => panic!(),
        Err(e) => {
            eprintln!("Starting upload session failed: {}", e);
            std::process::exit(2);
        }
    };

    let mut append_arg = files::UploadSessionAppendArg::new(
        files::UploadSessionCursor::new(session_id, 0));

    // Upload this many bytes in each request. The smaller this is, the more HTTP request overhead
    // there will be. But the larger it is, the more bandwidth that is potentially wasted on
    // network errors.
    const BUF_SIZE: usize = 32 * 1024 * 1024;

    // if the buffer is small we can stack-allocate it:
    //let mut buf = [0u8; BUF_SIZE];
    // otherwise it has to be heap-allocated:
    let mut buf = Vec::with_capacity(BUF_SIZE);
    buf.resize(BUF_SIZE, 0);

    let mut bytes_out = 0u64;
    let mut consecutive_errors = 0;
    let mut last_time = Instant::now();

    while consecutive_errors < 3 {
        let nread = source_file.read(&mut buf)
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

        match files::upload_session_append_v2(&client, &append_arg, &buf[0..nread]) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                eprintln!("Error appending data: {}", e);
                consecutive_errors += 1;
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
            Err(e) => {
                eprintln!("Error appending data: {}", e);
                consecutive_errors += 1;
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        }

        consecutive_errors = 0;

        bytes_out += nread as u64;
        append_arg.cursor.offset += nread as u64;

        let now = Instant::now();
        let time = now.duration_since(last_time);
        let millis = time.as_secs() * 1000 + time.subsec_millis() as u64;
        last_time = now;

        eprintln!("{}Bytes uploaded, {}Bytes per second",
                  human_number(bytes_out),
                  human_number(nread as u64 * 1000 / millis));
    }

    let finish = files::UploadSessionFinishArg::new(
        append_arg.cursor,
        files::CommitInfo::new(dest_path)
            .with_client_modified(Some(iso8601(source_mtime))));

    // TODO: Maybe should put a retry loop around this as well?
    match files::upload_session_finish(&client, &finish, &[]) {
        Ok(Ok(filemetadata)) => {
            println!("Upload succeeded!");
            println!("{:#?}", filemetadata);
        }
        Ok(Err(e)) => {
            eprintln!("Error finishing upload: {}", e);
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("Error finishing upload: {}", e);
            std::process::exit(2);
        }
    }
}
