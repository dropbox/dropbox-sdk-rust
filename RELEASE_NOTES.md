# v0.6.0
2020-09-24
* API update
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
