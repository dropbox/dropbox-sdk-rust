# v0.19.0-beta2
2024-11-05
* renamed sync_routes_default feature to sync_routes_in_root
* improved appearance of docs wrt features
* added tests for custom clients logic

# v0.19.0-beta1
2024-10-31
* **BIG CHANGE: async support added**
  * HTTP client traits completely redesigned
    * Should actually be much simpler to implement now, as the work of setting the right headers has been extracted out
      into client_helpers code, and implementations now just need to provide a generic HttpRequest type which can set
      any header.
  * The default sync client is still the default enabled feature.
  * To switch to the async mode, enable the `default_async_client` feature (and disable the `default_client`, `sync_routes`, `sync_routes_default` features).
* **BIG CHANGE: no more nested Results**
  * Functions which used to return `Result<Result<T, E>, dropbox_sdk::Error>` now return `Result<T, dropbox_sdk::Error<E>>`.
    * in other words, `Ok(Err(e))` is now written `Err(dropbox_sdk::Error::Api(e))` and `Ok(Ok(v))` is just `Ok(v)`.
    * `dropbox_sdk::Error` now has a type parameter which differs depending on the function being called.
* MSRV raised to 1.71.0

# v0.18.1
2024-05-06
* fixed bug when using oauth2 refresh tokens using client secret instead of PKCE:
    * See https://github.com/dropbox/dropbox-sdk-rust/issues/151 for details
    * New function Authorize::from_client_secret_refresh_token() should be used for any refresh tokens previously obtained using this flow
    * Thanks to Peerat Vichivanives for reporting and testing the fix

# v0.18.0
2024-01-12
* MSRV raised to 1.65.0
* oauth2 changes and bugfixes:
    * Fix the check for expired token in the default client.
    * Added a helper for the above: client_trait::is_token_expired_error()
    * Authorize::obtain_access_token() when currently holding a refresh token will return an updated bearer token and retain the refresh token. There was previously a bug where it would switch to only being a short-lived token with no way to refresh.
    * (breaking) Oauth2Type::AuthorizationCode is now a struct variant with a named field instead of a tuple variant.
    * (breaking) Oauth2Type::ImplicitGrant now is a nullary variant
    * (breaking) Renamed Authorization::from_access_token() to Authorization::from_long_lived_access_token() and marked it as deprecated

# v0.17.0
2023-11-10
* MSRV raised to 1.63.0
* Omit fields with null or default value in more places when serializing, and accept nulls when
  deserializing.
* Types which have an `extends` relationship in the spec now implement `From` to do conversion to
  or from the other type, as appropriate depending on the kind of types.

# v0.16.0
2023-08-13
* API spec update 2022-06-15, 2022-07-13, 2022-10-11, 2022-11-09, 2023-04-26
    * semver incompatible changes to structs:
      * files: TeamGetInfoResult, TeamMemberPolicies, TeamSharingPolicies
      * users: FullTeam, FullAccount
* Established 1.56.1 as the MSRV (minimum supported Rust version)
* Fix test generator under Python 3.11

# v0.15.0
2022-06-14
* API spec update 2022-02-09, 2022-03-02, 2022-03-30, 2022-04-20, 2022-05-18
    * semver incompatible changes in files:
        * upload: arg type changed from CommitInfo to UploadArg
        * upload, upload_session_append_v2, upload_session_append: error type changed from
            UploadSessionLookupError to UploadSessionAppendError
        * CommitInfoWithProperties removed (use UploadArg instead)
        * UploadErrorWithProperties removed (use UploadError instead)
    * files: new route upload_session_start_batch
    * files: upload routes can now take a content hash to validate upload contents
    * sharing: new route get_shared_link_metadata_app_auth
    * documentation, team_log updates
* Test improvements: routes (functions) all now have basic type signature checking tests

# v0.14.1
2022-02-09
* API spec update 2022-02-02
    * files: variants added to MoveIntoVaultError and UploadError
    * documentation, team_log updates

# v0.14.0
2021-12-09
* Unstable "Preview" functionality now gated behind an `unstable` feature flag.
* Codegen change to let rustc derive `Default` in most cases
* API spec update 2021-10-06, 2021-10-21, 2021-11-03, 2021-11-17, 2021-11-24, 2021-12-01
    * files::upload_session_finish_batch is deprecated by a "v2" version.
    * app auth added for files::list_folder and files::list_folder_continue
    * added preview file tagging functions
    * documentation, team_log updates

# v0.13.3
2021-09-29
* API spec update 2021-08-11, 2021-08-18, 2021-08-26, 2021-09-08, 2021-09-22, 2021-09-29
    * added files::upload_session_finish_batch_v2
    * documentation, team_log updates

# v0.13.2
2021-07-27
* Fixed a bug with non-ASCII paths using download and upload endpoints (#65)

# v0.13.1
2021-07-25
* Fixed doc generation on docs.rs. No code change.

# v0.13.0
2021-07-25
* Major update to OAuth2 code
  * Now supports short-lived tokens with refresh tokens
  * Now supports PKCE auth flow, for apps that can't have client secrets
* the `Error` type changed slightly:
  * `Error::InvalidToken` is now `Error::Authentication`, and uses the `auth::AuthError` type to
    specify the specific error, instead of a string.
  * `Error::AccessDenied` has been added which is used when handling HTTP 403 errors.
  * `Error::RateLimitedError` now uses the `auth::RateLimitReason` type to specify the specific
    error, instead of a string.
* the `auth` namespace is now always compiled in. The `dbx_auth` feature is retained, but ignored.

# v0.12.0
2021-07-10
* API spec updates:
  * 2021-05-11, 2021-05-26, 2021-06-16, 2021-06-23, 2021-07-02: comments and team_log namespace changes
  * 2021-06-30: (breaking change) remove deprecated sharing::change_file_member_access
    function and associated structs
  * 2021-07-08: sharing namespace additions
* Changed struct serialization to not emit fields at all where the value is `None`.
  * (previously it emitted `null` into the JSON as the value of the fields)

# v0.11.3
2021-05-08
* API spec updates:
  * 2021-05-07: team_members namespace
  * 2021-04-14: team namespace
  * 2021-04-08: team_log namespace

# v0.11.2
2021-03-26
* minor codegen change to make enum deserializers more concise
* API spec update 2021-03-03
  * documentation, team_log updates
* API spec update 2021-03-24
  * documentation, team_log updates

# v0.11.1
2021-02-22
* Routes tagged as deprecated in Stone now have the #[deprecated] attribute in Rust.
* API spec update 2021-02-18

# v0.11.0
2021-02-16
* Implemented path root selection for default_client. Clients now have a `.set_path_root()` 
  method which adds the `Dropbox-API-Path-Root` header, which sets what namespace paths are
  evaluated relative to. This requires that the `dbx_common` feature is enabled, as it uses the
  `PathRoot` type defined in that namespace.
* Updated to `ureq` 2.0.0. The only changes relevant to users of this crate is that it now uses
  `rustls` instead of system-native TLS, and that transport errors raised may be of higher quality.
* API spec update 2021-01-14
* API spec update 2021-01-22
* API spec update 2021-02-10
  * this contains a breaking change in the team_log namespace requiring a minor version bump: some
    fields that were previously required were made optional

# v0.10.0
2020-12-11
* Several improvements to error types:
  * Routes with no error type defined now return a new `NoError` type instead of `()`. This new
    type implements `Error`, so now all routes' error types are guaranteed to be `Error`, and
    hence, `Display` as well. This type is uninhabited, meaning you can never actually see
    instances of this type, it exists for type-checking purposes only.
  * Removed the deprecated `description` function from `Error` impl for error types.
  * Added a `cause()` method to the `Error` impl for error types if they have any variants which
    contain error values.
  * Improved the `Display` impl for `Error` types by using the documentation strings provided,
    including recursing into nested errors or values if present.
* API spec updates
  * 2020-12-03
  * 2020-11-12

# v0.9.0
2020-11-30
* All structs and enums except ones marked `union_closed` in the spec are now marked with
  `#[non_exhaustive]`. This means destructuring of structs must use `..` to indicate that other
  fields may be present, and matches on enums must include a `_` case to indicate that other
  variants may exist. This is because Stone explicitly reserves the right to add new fields and
  variants unless the type is a `union_closed`, so adding this attribute means future updates won't
  break existing code.
* All structs and enums now implement the `PartialEq` and `Clone` traits, in addition to `Debug`
  which was already implemented.
* Renamed the `_Unknown` catch-all enum variant to `Other` in all cases. Going along with the above,
  your match statement should always have an `Other | _ => { ... }` case at the end. `Other`
  protects you against server-side changes, whereas `_` protects you against client-side code
  changes.
* `generate.sh` shell script replaced with python code, `generate.py`.
* added `update_spec.py` which updates the Stone API spec and tells you whether the update is semver
  compatible or not.
* Test improvements: all enum variants now have generated test cases. This increases the number of
  tests by roughly a factor of 4.

# v0.8.0
2020-11-11
* Improvements to builder methods on structs (the `fn with_fieldname(self, ...) -> Self` methods)
  * Generate builder methods for structs which have only optional fields. These were erroneously
    missing.
  * Change builder methods for `Optional<T>` fields to take the inner type `T` as the argument
    directly. Users will need to remove `Some( ... )` from the current argument for these methods.
* API spec update 2020-11-06
  * Adds parallel chunked uploads!

# v0.7.1
2020-11-05
* Documentation updates only.

# v0.7.0
2020-10-28
* Implemented support for different authentication types.
  * `HyperClient` is now `UserAuthHyperClient`.
  * API methods now take a different type of client for their first argument, depending on the auth
    type they require.
* Moved OAuth2 helper code out of hyper_client and made it independent of the HTTP client used, so
  users of custom HTTP clients don't have to reinvent the wheel for OAuth2.
  * The `oauth2_token_from_authorization_code` now is in a different module, and takes a HTTP
    client implementation as a new first argument.
* Changed the Error::RateLimited variant to include the requested backoff time.
* Replaced default HTTP client's hyper 0.10 implementation with one based on `ureq`.
  * Hyper 0.10 is out of date and unsupported, but we're not ready to transition to async style,
    which rules out upgrading to newer Hyper versions.
  * `ureq` as a small, synchronous HTTP client that is actively maintained.
  * There are some name changes, generally replacing "hyper" with "default", so that the
    implementation can be changed in the future without renaming things again:
    * `hyper_client` module -> `default_client`
    * `hyper_client` Cargo feature -> `default_client`
    * `UserAuthHyperClient` -> `UserAuthDefaultClient`
    * `TeamAuthHyperClient` -> `TeamAuthDefaultClient`
    * ... etc.
* API spec updates
  * 2020-10-28
  * 2020-10-15
  * 2020-10-09

# v0.6.0
2020-09-24
* API update 2020-09-17
  * notably, the `cloud_docs` namespace has been removed.
* add Dropbox-standard meta files
* enabled Dependabot

# v0.5.0
2020-08-17
* change CI to GitHub Actions
* update Stone to 2.0.0

## Breaking Change: Removing `error_chain`
Since the beginning, dropbox-sdk has used `error_chain` to generate the `Error`
type used in the return value of all routes. This crate has been
semi-deprecated and replaced in most codebases by either `thiserror` (for
libraries) or `anyhow` (for applications). This release replaces `error_chain`
with `thiserror`, which is a more "vanilla" error type crate, in that it
doesn't appear in the types it generates, it simply eliminates the boilerplate
code associated with writing an error type. This means it should integrate
easily with whatever error-handling strategy your code uses.

### Migration Advice
The change removing `error_chain` does not significantly change the Error
enum's variants, but it does mean that users can't use the `.chain_err(|| ...)`
method on it. Users who found that functionality useful are encouraged to take
a look at the [`anyhow`](https://github.com/dtolnay/anyhow) crate which
provides similar functionality.

Alternatively, you can keep using `error_chain` in your code and include
dropbox_sdk in your error type's `foreign_links` section.

# v0.4.0
2020-06-26
*  API update
*  example code fixes

# v0.3.0
2020-04-17
* API update
* example code fixes

# v0.2.0
2019-12-16
* API update
* improvements to rustdoc formatting

# v0.1.0
2019-09-06
* initial public release
