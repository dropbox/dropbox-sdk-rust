use dropbox_sdk::files;
use dropbox_sdk::client_trait::UserAuthClient;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use threadpool::ThreadPool;

pub fn create_files(
    client: Arc<impl UserAuthClient + Send + Sync + 'static>,
    path: &'static str,
    num_files: u32,
    size: usize,
) -> (Box<impl Fn(u32) -> String>, Box<impl Fn(u32) -> Vec<u8>>) {
    let threadpool = ThreadPool::new(20);

    let file_bytes = move |i| format!("This is file {}.\n", i)
        .into_bytes()
        .into_iter()
        .cycle()
        .take(size)
        .collect::<Vec<u8>>();
    let file_path = move |i| format!("{}/file{}.txt", path, i);

    println!("Creating {} files in {}", num_files, path);
    for i in 0 .. num_files {
        let c = client.clone();
        threadpool.execute(move || {
            let path = file_path(i);
            let arg = files::CommitInfo::new(path.clone())
                .with_mode(files::WriteMode::Overwrite);
            loop {
                println!("{}: writing", path);
                match files::upload(c.as_ref(), &arg, &file_bytes(i)) {
                    Ok(Ok(_)) => (),
                    Err(dropbox_sdk::Error::RateLimited { retry_after_seconds, .. }) => {
                        println!("{}: rate limited; sleeping {} seconds",
                            path, retry_after_seconds);
                        thread::sleep(Duration::from_secs(retry_after_seconds as u64));
                        continue;
                    }
                    e => panic!("{}: upload failed: {:?}", path, e),
                }
                println!("{}: done", path);
                break;
            }
        });
    }

    threadpool.join();
    (Box::new(file_path), Box::new(file_bytes))
}

pub fn create_clean_folder(client: &impl UserAuthClient, path: &str) {
    println!("Deleting any existing {} folder", path);
    match files::delete_v2(client, &files::DeleteArg::new(path.to_owned())) {
        Ok(Ok(_)) | Ok(Err(files::DeleteError::PathLookup(files::LookupError::NotFound))) => (),
        e => panic!("unexpected result when deleting {}: {:?}", path, e),
    }

    println!("Creating folder {}", path);
    match files::create_folder_v2(
        client, &files::CreateFolderArg::new(path.to_owned()).with_autorename(false))
    {
        Ok(Ok(_)) => (),
        e => panic!("unexpected result when creating {}: {:?}", path, e),
    }
}
