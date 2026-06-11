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
        "name": "spaghetti",
        "some extra field": "whatever",
        "some more": {"some": "complex", "other": "stuff"},
        "path_lower": "/pasta/spaghetti",
        "path_display": "/PASTA/spaghetti",
        "one more extra": "~~~~"
    }"#;
    let x = serde_json::from_str::<dropbox_sdk::files::Metadata>(json).unwrap();
    if let dropbox_sdk::files::Metadata::Deleted(d) = x {
        assert_eq!("spaghetti", d.name);
        assert_eq!(Some("/pasta/spaghetti"), d.path_lower.as_deref());
        assert_eq!(Some("/PASTA/spaghetti"), d.path_display.as_deref());
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
