use dropbox_sdk::client_trait::UserAuthClient;
use dropbox_sdk::files;
use dropbox_sdk::Error::Api;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use threadpool::ThreadPool;

pub fn create_files(
    client: Arc<impl UserAuthClient + Send + Sync + 'static>,
    path: &'static str,
    num_files: u32,
    size: usize,
) -> (impl Fn(u32) -> String, impl Fn(u32) -> Vec<u8>) {
    let threadpool = ThreadPool::new(20);

    let file_bytes = move |i| {
        format!("This is file {i}.\n")
            .into_bytes()
            .into_iter()
            .cycle()
            .take(size)
            .collect::<Vec<u8>>()
    };
    let file_path = move |i| format!("{path}/file{i}.txt");

    println!("Creating {num_files} files in {path}");
    for i in 0..num_files {
        let c = client.clone();
        threadpool.execute(move || {
            let path = file_path(i);
            let arg = files::UploadArg::new(path.clone()).with_mode(files::WriteMode::Overwrite);
            loop {
                println!("{path}: writing");
                match files::upload(c.as_ref(), &arg, &file_bytes(i)) {
                    Ok(_) => (),
                    Err(dropbox_sdk::Error::RateLimited {
                        retry_after_seconds,
                        ..
                    }) => {
                        println!("{path}: rate limited; sleeping {retry_after_seconds} seconds");
                        thread::sleep(Duration::from_secs(retry_after_seconds as u64));
                        continue;
                    }
                    Err(e) => panic!("{path}: upload failed: {e:?}"),
                }
                println!("{path}: done");
                break;
            }
        });
    }

    threadpool.join();
    (file_path, file_bytes)
}

pub fn create_clean_folder(client: &impl UserAuthClient, path: &str) {
    println!("Deleting any existing {path} folder");
    match files::delete_v2(client, &files::DeleteArg::new(path.to_owned())) {
        Ok(_) | Err(Api(files::DeleteError::PathLookup(files::LookupError::NotFound))) => (),
        Err(e) => panic!("unexpected result when deleting {path}: {e:?}"),
    }

    println!("Creating folder {path}");
    match files::create_folder_v2(
        client,
        &files::CreateFolderArg::new(path.to_owned()).with_autorename(false),
    ) {
        Ok(_) => (),
        Err(e) => panic!("unexpected result when creating {path}: {e:?}"),
    }
}
