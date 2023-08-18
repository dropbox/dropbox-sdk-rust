#[test]
fn test_null_fields_elided() {
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
    });

    let s = serde_json::to_string_pretty(&value).unwrap();
    let deser = serde_json::from_str::<serde_json::Value>(&s).unwrap();
    assert_eq!(expected, deser);

    // Make sure deserializing it also works and we get our starting value back.
    let roundtrip = serde_json::from_str::<dropbox_sdk::files::Metadata>(&s).unwrap();
    assert_eq!(roundtrip, value);
}
