#![warn(rust_2018_idioms)]

//
// Various tests for forward-compatibility.
// Stone explicitly allows receiving structures with extra fields the receiver doesn't know about
// (except for closed unions).
//

#[test]
fn test_extra_fields() {
    let json = r#"{
        ".tag": "deleted",
        "name": "f",
        "some extra field": "whatever",
        "some more": {"some": "complex", "other": "stuff"},
        "parent_shared_folder_id": "spaghetti",
        "one more extra": "~~~~"
    }"#;
    let x = serde_json::from_str::<dropbox_sdk::files::Metadata>(json).unwrap();
    if let dropbox_sdk::files::Metadata::Deleted(d) = x {
        assert_eq!("f", &d.name);
        assert_eq!(Some("spaghetti"), d.parent_shared_folder_id.as_deref());
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_open_union_void() {
    let json = r#"{
        ".tag": "some other variant",
        "some field": "some value"
    }"#;
    let x = serde_json::from_str::<dropbox_sdk::files::ListFolderLongpollError>(json).unwrap();
    if let dropbox_sdk::files::ListFolderLongpollError::Other = x {
        // yay
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_open_union_fields() {
    let json = r#"{
        ".tag": "some other variant",
        "some field": "some value",
        "another field": "another value"
    }"#;
    let x = serde_json::from_str::<dropbox_sdk::users::SpaceAllocation>(json).unwrap();
    if let dropbox_sdk::users::SpaceAllocation::Other = x {
        // yay
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_open_union_new_field() {
    let json = r#"{
        ".tag": "individual",
        "allocated": 9999,
        "something else": "some value"
    }"#;
    let x = serde_json::from_str::<dropbox_sdk::users::SpaceAllocation>(json).unwrap();
    if let dropbox_sdk::users::SpaceAllocation::Individual(indiv) = x {
        if indiv.allocated != 9999 {
            panic!("wrong value");
        }
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_void_union_with_fields() {
    let json = r#"{
        ".tag": "reset",
        "some field": "some value",
        "another field": "another value"
    }"#;
    let x = serde_json::from_str::<dropbox_sdk::files::ListFolderLongpollError>(json).unwrap();
    if let dropbox_sdk::files::ListFolderLongpollError::Reset = x {
        // yay
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_open_polymorphic_struct() {
    let json = r#"{
        ".tag": "some other variant",
        "root_namespace_id": "12345",
        "home_namespace_id": "67890"
    }"#;
    let x = serde_json::from_str::<dropbox_sdk::common::RootInfo>(json).unwrap();
    if let dropbox_sdk::common::RootInfo::Other = x {
        // yay
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_null_fields_elided() {
    // This isn't really a compatibility test, but...
    // Struct fields with optional or default values don't need to be serialized.
    let value = dropbox_sdk::files::Metadata::File(
        dropbox_sdk::files::FileMetadata::new(
            "name".to_owned(),
            "id".to_owned(),
            "client_modified".to_owned(),
            "server_modified".to_owned(),
            "rev".to_owned(),
            1337)
        // Many other optional fields not populated.
    );

    // Serialized value should only contain these fields, and should not have the optional fields
    // included, like `"media_info": null`.
    let expected = serde_json::json!({
        ".tag": "file",
        "name": "name",
        "id": "id",
        "client_modified": "client_modified",
        "server_modified": "server_modified",
        "rev": "rev",
        "size": 1337,
        // FIXME(wfraser): this field is a default, it should not be included either
        "is_downloadable": true,
    });
    let s = serde_json::to_string_pretty(&value).unwrap();
    let deser = serde_json::from_str::<serde_json::Value>(&s).unwrap();
    assert_eq!(expected, deser);

    // Make sure deserializing it also works and we get our starting value back.
    let roundtrip = serde_json::from_str::<dropbox_sdk::files::Metadata>(&s).unwrap();
    assert_eq!(roundtrip, value);
}
