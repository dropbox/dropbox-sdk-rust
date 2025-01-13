#![deny(rust_2018_idioms)]

//! This example illustrates advanced usage of Dropbox's chunked file upload API to upload large
//! files that would not fit in a single HTTP request, including allowing the user to resume
//! interrupted uploads, and uploading blocks in parallel.

use dropbox_sdk::default_client::UserAuthDefaultClient;
use dropbox_sdk::files;
use dropbox_sdk::Error::Api;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

/// How many blocks to upload in parallel.
const PARALLELISM: usize = 20;

/// The size of a block. This is a Dropbox constant, not adjustable.
const BLOCK_SIZE: usize = 4 * 1024 * 1024;

/// We can upload an integer multiple of BLOCK_SIZE in a single request. This reduces the number of
/// requests needed to do the upload and can help avoid running into rate limits.
const BLOCKS_PER_REQUEST: usize = 2;

macro_rules! fatal {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
        exit(2);
    }
}

fn usage() {
    eprintln!(
        "usage: {} <source file path> <Dropbox path> [--resume <session ID>,<resume offset>]",
        std::env::args().next().unwrap()
    );
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

#[derive(Debug, Clone)]
struct Resume {
    start_offset: u64,
    session_id: String,
}

impl std::str::FromStr for Resume {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.rsplitn(2, ',');
        let offset_str = parts.next().ok_or("missing session ID and file offset")?;
        let session_id = parts.next().ok_or("missing file offset")?.to_owned();
        let start_offset = offset_str.parse().map_err(|_| "invalid file offset")?;
        Ok(Self {
            start_offset,
            session_id,
        })
    }
}

fn parse_args() -> Operation {
    let mut a = std::env::args().skip(1);
    match (a.next(), a.next()) {
        (Some(ref arg), _) if arg == "--help" || arg == "-h" => Operation::Usage,
        (Some(src), Some(dest)) => {
            let resume = match (a.next().as_deref(), a.next()) {
                (Some("--resume"), Some(resume_str)) => match resume_str.parse() {
                    Ok(resume) => Some(resume),
                    Err(e) => {
                        eprintln!("Invalid --resume argument: {}", e);
                        return Operation::Usage;
                    }
                },
                (None, _) => None,
                _ => {
                    return Operation::Usage;
                }
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
        (None, _) => Operation::Usage,
    }
}

/// Figure out if destination is a folder or not and change the destination path accordingly.
fn get_destination_path(
    client: &UserAuthDefaultClient,
    given_path: &str,
    source_path: &Path,
) -> Result<String, String> {
    let filename = source_path
        .file_name()
        .ok_or_else(|| format!("invalid source path {:?} has no filename", source_path))?
        .to_string_lossy();

    // Special-case: we can't get metadata for the root, so just use the source path filename.
    if given_path == "/" {
        let mut path = "/".to_owned();
        path.push_str(&filename);
        return Ok(path);
    }

    let meta_result =
        files::get_metadata(client, &files::GetMetadataArg::new(given_path.to_owned()));

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
        Err(Api(files::GetMetadataError::Path(files::LookupError::NotFound))) => {
            // Given destination path doesn't exist, which is just fine. Use the given path as-is.
            // Note that it's fine if the path's parents don't exist either; folders will be
            // automatically created as needed.
            Ok(given_path.to_owned())
        }
        Err(e) => Err(format!("Error looking up destination: {}", e)),
    }
}

/// Keep track of some shared state accessed / updated by various parts of the uploading process.
struct UploadSession {
    session_id: String,
    start_offset: u64,
    file_size: u64,
    bytes_transferred: AtomicU64,
    completion: Mutex<CompletionTracker>,
}

impl UploadSession {
    /// Make a new upload session.
    pub fn new(client: &UserAuthDefaultClient, file_size: u64) -> Result<Self, String> {
        let session_id = match files::upload_session_start(
            client,
            &files::UploadSessionStartArg::default()
                .with_session_type(files::UploadSessionType::Concurrent),
            &[],
        ) {
            Ok(result) => result.session_id,
            Err(e) => return Err(format!("Starting upload session failed: {:?}", e)),
        };

        Ok(Self {
            session_id,
            start_offset: 0,
            file_size,
            bytes_transferred: AtomicU64::new(0),
            completion: Mutex::new(CompletionTracker::default()),
        })
    }

    /// Resume a pre-existing (i.e. interrupted) upload session.
    pub fn resume(resume: Resume, file_size: u64) -> Self {
        Self {
            session_id: resume.session_id,
            start_offset: resume.start_offset,
            file_size,
            bytes_transferred: AtomicU64::new(0),
            completion: Mutex::new(CompletionTracker::resume_from(resume.start_offset)),
        }
    }

    /// Generate the argument to append a block at the given offset.
    pub fn append_arg(&self, block_offset: u64) -> files::UploadSessionAppendArg {
        files::UploadSessionAppendArg::new(files::UploadSessionCursor::new(
            self.session_id.clone(),
            self.start_offset + block_offset,
        ))
    }

    /// Generate the argument to commit the upload at the given path with the given modification
    /// time.
    pub fn commit_arg(
        &self,
        dest_path: String,
        source_mtime: SystemTime,
    ) -> files::UploadSessionFinishArg {
        files::UploadSessionFinishArg::new(
            files::UploadSessionCursor::new(self.session_id.clone(), self.file_size),
            files::CommitInfo::new(dest_path).with_client_modified(iso8601(source_mtime)),
        )
    }

    /// Mark a block as uploaded.
    pub fn mark_block_uploaded(&self, block_offset: u64, block_len: u64) {
        let mut completion = self.completion.lock().unwrap();
        completion.complete_block(self.start_offset + block_offset, block_len);
    }

    /// Return the offset up to which the file is completely uploaded. It can be resumed from this
    /// position if something goes wrong.
    pub fn complete_up_to(&self) -> u64 {
        let completion = self.completion.lock().unwrap();
        completion.complete_up_to
    }
}

/// Because blocks can be uploaded out of order, if an error is encountered when uploading a given
/// block, that is not necessarily the correct place to resume uploading from next time: there may
/// be gaps before that block.
///
/// This struct is for keeping track of what offset the file has been completely uploaded to.
///
/// When a block is finished uploading, call `complete_block` with the offset and length.
#[derive(Default)]
struct CompletionTracker {
    complete_up_to: u64,
    uploaded_blocks: HashMap<u64, u64>,
}

impl CompletionTracker {
    /// Make a new CompletionTracker that assumes everything up to the given offset is complete. Use
    /// this if resuming a previously interrupted session.
    pub fn resume_from(complete_up_to: u64) -> Self {
        Self {
            complete_up_to,
            uploaded_blocks: HashMap::new(),
        }
    }

    /// Mark a block as completely uploaded.
    pub fn complete_block(&mut self, block_offset: u64, block_len: u64) {
        if block_offset == self.complete_up_to {
            // Advance the cursor.
            self.complete_up_to += block_len;

            // Also look if we can advance it further still.
            while let Some(len) = self.uploaded_blocks.remove(&self.complete_up_to) {
                self.complete_up_to += len;
            }
        } else {
            // This block isn't at the low-water mark; there's a gap behind it. Save it for later.
            self.uploaded_blocks.insert(block_offset, block_len);
        }
    }
}

fn get_file_mtime_and_size(f: &File) -> Result<(SystemTime, u64), String> {
    let meta = f
        .metadata()
        .map_err(|e| format!("Error getting source file metadata: {}", e))?;
    let mtime = meta
        .modified()
        .map_err(|e| format!("Error getting source file mtime: {}", e))?;
    Ok((mtime, meta.len()))
}

/// This function does it all.
fn upload_file(
    client: Arc<UserAuthDefaultClient>,
    mut source_file: File,
    dest_path: String,
    resume: Option<Resume>,
) -> Result<(), String> {
    let (source_mtime, source_len) = get_file_mtime_and_size(&source_file)?;

    let session = Arc::new(if let Some(ref resume) = resume {
        source_file
            .seek(SeekFrom::Start(resume.start_offset))
            .map_err(|e| format!("Seek error: {}", e))?;
        UploadSession::resume(resume.clone(), source_len)
    } else {
        UploadSession::new(client.as_ref(), source_len)?
    });

    eprintln!("upload session ID is {}", session.session_id);

    // Initially set to the end of the file and an empty block; if the file is an exact multiple of
    // BLOCK_SIZE, we'll need to upload an empty buffer when closing the session.
    let last_block = Arc::new(Mutex::new((source_len, vec![])));

    let start_time = Instant::now();
    let upload_result = {
        let client = client.clone();
        let session = session.clone();
        let last_block = last_block.clone();
        let resume = resume.clone();
        parallel_reader::read_stream_and_process_chunks_in_parallel(
            &mut source_file,
            BLOCK_SIZE * BLOCKS_PER_REQUEST,
            PARALLELISM,
            Arc::new(move |block_offset, data: &[u8]| -> Result<(), String> {
                let append_arg = session.append_arg(block_offset);
                if data.len() != BLOCK_SIZE * BLOCKS_PER_REQUEST {
                    // This must be the last block. Only the last one is allowed to be not 4 MiB
                    // exactly. Save the block and offset so it can be uploaded after all the
                    // parallel uploads are done. This is because once the session is closed, we
                    // can't resume it.
                    let mut last_block = last_block.lock().unwrap();
                    last_block.0 = block_offset + session.start_offset;
                    last_block.1 = data.to_vec();
                    return Ok(());
                }
                let result = upload_block_with_retry(
                    client.as_ref(),
                    &append_arg,
                    data,
                    start_time,
                    session.as_ref(),
                    resume.as_ref(),
                );
                if result.is_ok() {
                    session.mark_block_uploaded(block_offset, data.len() as u64);
                }
                result
            }),
        )
    };

    if let Err(e) = upload_result {
        return Err(format!(
            "{}. To resume, use --resume {},{}",
            e,
            session.session_id,
            session.complete_up_to()
        ));
    }

    let (last_block_offset, last_block_data) = unwrap_arcmutex(last_block);
    eprintln!(
        "closing session at {} with {}-byte block",
        last_block_offset,
        last_block_data.len()
    );
    let mut arg = session.append_arg(last_block_offset);
    arg.close = true;
    if let Err(e) = upload_block_with_retry(
        client.as_ref(),
        &arg,
        &last_block_data,
        start_time,
        session.as_ref(),
        resume.as_ref(),
    ) {
        eprintln!("failed to close session: {}", e);
        // But don't error out; try committing anyway. It could be we're resuming a file where we
        // already closed it out but failed to commit.
    }

    eprintln!("committing...");
    let finish = session.commit_arg(dest_path, source_mtime);

    let mut retry = 0;
    while retry < 3 {
        match files::upload_session_finish(client.as_ref(), &finish, &[]) {
            Ok(file_metadata) => {
                println!("Upload succeeded!");
                println!("{:#?}", file_metadata);
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error finishing upload: {:?}", e);
                retry += 1;
                sleep(Duration::from_secs(1));
            }
        }
    }

    Err(format!(
        "Upload failed. To retry, use --resume {},{}",
        session.session_id,
        session.complete_up_to()
    ))
}

/// Upload a single block, retrying a few times if an error occurs.
///
/// Prints progress and upload speed, and updates the UploadSession if successful.
fn upload_block_with_retry(
    client: &UserAuthDefaultClient,
    arg: &files::UploadSessionAppendArg,
    buf: &[u8],
    start_time: Instant,
    session: &UploadSession,
    resume: Option<&Resume>,
) -> Result<(), String> {
    let block_start_time = Instant::now();
    let mut errors = 0;
    loop {
        match files::upload_session_append_v2(client, arg, buf) {
            Ok(()) => {
                break;
            }
            Err(dropbox_sdk::Error::RateLimited {
                reason,
                retry_after_seconds,
            }) => {
                eprintln!("rate-limited ({reason}), waiting {retry_after_seconds} seconds");
                if retry_after_seconds > 0 {
                    sleep(Duration::from_secs(u64::from(retry_after_seconds)));
                }
            }
            Err(error) => {
                errors += 1;
                let msg = format!("Error calling upload_session_append: {error:?}");
                if errors == 3 {
                    return Err(msg);
                } else {
                    eprintln!("{}; retrying...", msg);
                }
            }
        }
    }

    let now = Instant::now();
    let block_dur = now.duration_since(block_start_time);
    let overall_dur = now.duration_since(start_time);

    let block_bytes = buf.len() as u64;
    let bytes_sofar = session.bytes_transferred.fetch_add(block_bytes, SeqCst) + block_bytes;

    let percent = (resume.map(|r| r.start_offset).unwrap_or(0) + bytes_sofar) as f64
        / session.file_size as f64
        * 100.;

    // This assumes that we have `PARALLELISM` uploads going at the same time and at roughly the
    // same upload speed:
    let block_rate = block_bytes as f64 / block_dur.as_secs_f64() * PARALLELISM as f64;

    let overall_rate = bytes_sofar as f64 / overall_dur.as_secs_f64();

    eprintln!(
        "{:.01}%: {}Bytes uploaded, {}Bytes per second, {}Bytes per second average",
        percent,
        human_number(bytes_sofar),
        human_number(block_rate as u64),
        human_number(overall_rate as u64),
    );

    Ok(())
}

fn human_number(n: u64) -> String {
    let mut f = n as f64;
    let prefixes = ['k', 'M', 'G', 'T', 'E'];
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

    chrono::DateTime::from_timestamp(timestamp, 0 /* nsecs */)
        .expect("invalid or out-of-range timestamp")
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

fn unwrap_arcmutex<T: std::fmt::Debug>(x: Arc<Mutex<T>>) -> T {
    Arc::try_unwrap(x)
        .expect("failed to unwrap Arc")
        .into_inner()
        .expect("failed to unwrap Mutex")
}

fn main() {
    env_logger::init();

    let args = match parse_args() {
        Operation::Usage => {
            usage();
            exit(1);
        }
        Operation::Upload(args) => args,
    };

    let source_file = File::open(&args.source_path).unwrap_or_else(|e| {
        fatal!("Source file {:?} not found: {}", args.source_path, e);
    });

    let auth = dropbox_sdk::oauth2::get_auth_from_env_or_prompt();
    let client = Arc::new(UserAuthDefaultClient::new(auth));

    let dest_path = get_destination_path(client.as_ref(), &args.dest_path, &args.source_path)
        .unwrap_or_else(|e| {
            fatal!("Error: {}", e);
        });

    eprintln!("source = {:?}", args.source_path);
    eprintln!("dest   = {:?}", dest_path);

    upload_file(client, source_file, dest_path, args.resume).unwrap_or_else(|e| {
        fatal!("{}", e);
    });
}
