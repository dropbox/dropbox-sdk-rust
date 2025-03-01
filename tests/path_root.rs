use dropbox_sdk::common::PathRoot;
use dropbox_sdk::default_client::UserAuthDefaultClient;
use dropbox_sdk::files::{self, ListFolderArg};

#[test]
#[ignore] // requires a pre-configured app token; should be run separately
fn invalid_path_root() {
    let auth = dropbox_sdk::oauth2::get_auth_from_env_or_prompt();
    let mut client = UserAuthDefaultClient::new(auth);
    client.set_path_root(&PathRoot::NamespaceId("1".to_owned()));
    match files::list_folder(&client, &ListFolderArg::new("/".to_owned())) {
        // If the oauth token is for an app which only has access to its app folder, then the path
        // root cannot be specified.
        Err(dropbox_sdk::Error::BadRequest(msg))
            if msg.contains("Path root is not supported for sandbox app") => {}

        // If the oauth token is for a "whole dropbox" app, then we should get this error, which
        // inside will have a "no_permission" error.
        // If the error is due to a change in the user's home nsid, then we get an "invalid_root"
        // error which includes the new nsid, but that's not what we expect here, where we're just
        // giving a bogus nsid.
        Err(dropbox_sdk::Error::UnexpectedHttpError {
            code: 422,
            response,
            ..
        }) => {
            let error = serde_json::from_str::<serde_json::Value>(&response)
                .unwrap_or_else(|e| panic!("invalid json {:?}: {}", response, e));
            let tag = error
                .as_object()
                .and_then(|map| map.get("error"))
                .and_then(|v| v.as_object())
                .and_then(|map| map.get(".tag"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("got a weird error {}", response));

            if tag != "no_permission" {
                panic!("unexpected error kind in {:?}", response);
            }
        }

        // Any other result is a bug.
        otherwise => panic!("wrong result: {:?}", otherwise),
    }
}
