// until tool_lints is stable, we can't use the 'clippy::' prefix on warnings, so we have to
// silence the warning about THAT...
#![cfg_attr(feature = "cargo-clippy", allow(renamed_and_removed_lints))]

extern crate dropbox_sdk;
extern crate serde_json;

mod generated; // You need to run the Stone generator to create this module.
