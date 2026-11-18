use std::path::PathBuf;

use gds::service::remote::Address;

use super::{Target, classify_uri_payload, renamed_remote_address};

#[test]
fn item_targets_group_by_parent_and_deduplicate_selections() {
    let targets = Target::items(
        ["/tmp/a", "/", "/tmp/folder", "/tmp/a"]
            .map(PathBuf::from)
            .to_vec(),
    );
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].directory, PathBuf::from("/tmp"));
    assert_eq!(
        targets[0].selection,
        [PathBuf::from("/tmp/a"), PathBuf::from("/tmp/folder")]
    );
    assert_eq!(targets[1].directory, PathBuf::from("/"));
    assert_eq!(targets[1].selection, [PathBuf::from("/")]);
}

#[test]
fn folder_targets_open_each_directory_once_without_selection() {
    let targets = Target::folders(
        ["/tmp/folder", "/", "/tmp/folder"]
            .map(PathBuf::from)
            .to_vec(),
    );
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].directory, PathBuf::from("/tmp/folder"));
    assert!(targets[0].selection.is_empty());
    assert_eq!(targets[1].directory, PathBuf::from("/"));
    assert!(targets[1].selection.is_empty());
}

#[test]
fn classify_uri_payload_separates_local_and_remote_uris() {
    let payload = classify_uri_payload(&[
        "file:///tmp/demo.txt".to_owned(),
        "sftp://alice@example.com/srv/notes.md".to_owned(),
        "/tmp/plain".to_owned(),
    ]);
    assert_eq!(
        payload.local,
        vec![PathBuf::from("/tmp/demo.txt"), PathBuf::from("/tmp/plain")]
    );
    assert_eq!(payload.remote.len(), 1);
    assert_eq!(payload.remote[0].host(), "example.com");
}

#[test]
fn renamed_remote_address_validates_and_replaces_leaf_preserving_host() {
    let Ok(addr) = Address::parse("sftp://alice@example.com/srv/old.txt") else {
        panic!("valid remote test address");
    };
    let Ok(renamed) = renamed_remote_address(&addr, "new.txt") else {
        panic!("valid renamed target");
    };
    assert_eq!(renamed.name(), "new.txt");
    assert_eq!(renamed.host(), "example.com");
    assert!(renamed_remote_address(&addr, "bad/name").is_err());
}
