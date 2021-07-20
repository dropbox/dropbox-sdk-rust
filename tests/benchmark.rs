use dropbox_sdk::files;
use dropbox_sdk::default_client::UserAuthDefaultClient;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use threadpool::ThreadPool;

mod common;

/// This test should be run with --nocapture to see the timing info output.
#[test]
#[ignore] // very time-consuming to run; should be run separately
fn fetch_files() {
    let auth = dropbox_sdk::oauth2::get_auth_from_env_or_prompt();
    let client = Arc::new(UserAuthDefaultClient::new(auth));
    let threadpool = ThreadPool::new(20);

    const FOLDER: &str = "/fetch_small_files";
    const NUM_FILES: u32 = 100;
    const NUM_TEST_RUNS: u32 = 4;
    const FILE_SIZE: usize = 1024 * 1024; // 1 MiB

    println!("Setting up test environment");

    common::create_clean_folder(client.as_ref(), FOLDER);

    let (file_path, file_bytes) = common::create_files(
        client.clone(), FOLDER, NUM_FILES, FILE_SIZE);

    threadpool.join();

    println!("Test setup complete. Starting benchmark.");

    let mut times = vec![];
    for _ in 0 .. NUM_TEST_RUNS {
        println!("sleeping 10 seconds before run");
        thread::sleep(Duration::from_secs(10));
        let start = Instant::now();
        for i in 0 .. NUM_FILES {
            let path = file_path(i);
            let expected_bytes = file_bytes(i);
            let c = client.clone();
            threadpool.execute(move || {
                loop {
                    let arg = files::DownloadArg::new(path.clone());
                    match files::download(c.as_ref(), &arg, None, None) {
                        Ok(Ok(result)) => {
                            let mut read_bytes = Vec::new();
                            result.body.expect("result should have a body")
                                .read_to_end(&mut read_bytes).expect("read_to_end");
                            assert_eq!(&read_bytes, &expected_bytes);
                        }
                        Err(dropbox_sdk::Error::RateLimited { retry_after_seconds, .. }) => {
                            eprintln!("WARNING: rate-limited {} seconds", retry_after_seconds);
                            thread::sleep(Duration::from_secs(retry_after_seconds as u64));
                            continue;
                        }
                        Ok(Err(e)) => panic!("{}: download failed: {:?}", path, e),
                        Err(e) => panic!("{}: download failed: {:?}", path, e),
                    }
                    break;
                }
            });
        }

        threadpool.join();
        let dur = start.elapsed();
        println!("test finished in {} seconds", dur.as_secs_f64());
        times.push(dur);
    }

    println!("{:?}", times);
    println!("average: {} seconds",
        times.iter().map(Duration::as_secs_f64).sum::<f64>() / times.len() as f64)
}
