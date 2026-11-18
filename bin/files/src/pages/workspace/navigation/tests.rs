use std::time::{Duration, UNIX_EPOCH};

use gds::service::remote::{Address, Entry as RemoteEntry, EntryKind as RemoteKind};

use super::sort_remote_entries;
use crate::entities::file::sorting::{Direction, Key, Sort};

fn remote_item(name: &str, kind: RemoteKind, size: Option<u64>, secs: u64) -> RemoteEntry {
    let Ok(base) = Address::parse("sftp://user@example.com/root") else {
        panic!("valid remote test address");
    };
    let Ok(address) = base.child(name) else {
        panic!("valid remote child");
    };
    RemoteEntry {
        address,
        name: name.to_owned(),
        kind,
        size,
        modified: Some(UNIX_EPOCH + Duration::from_secs(secs)),
    }
}

#[test]
fn remote_sort_keeps_directories_first_and_ties_stable() {
    let mut entries = vec![
        remote_item("zeta.txt", RemoteKind::File, Some(5), 10),
        remote_item("docs", RemoteKind::Directory, None, 5),
        remote_item("alpha.txt", RemoteKind::File, Some(5), 20),
    ];
    sort_remote_entries(
        &mut entries,
        Sort {
            key: Key::Size,
            direction: Direction::Ascending,
        },
    );
    assert_eq!(entries[0].name, "docs");
    assert_eq!(entries[1].name, "alpha.txt");
    assert_eq!(entries[2].name, "zeta.txt");
}

#[test]
fn remote_sort_descending_keeps_directories_first_and_reverses_files() {
    let mut entries = vec![
        remote_item("small.txt", RemoteKind::File, Some(5), 10),
        remote_item("dir-b", RemoteKind::Directory, None, 5),
        remote_item("large.txt", RemoteKind::File, Some(50), 20),
        remote_item("dir-a", RemoteKind::Directory, None, 15),
    ];
    sort_remote_entries(
        &mut entries,
        Sort {
            key: Key::Size,
            direction: Direction::Descending,
        },
    );
    assert_eq!(entries[0].name, "dir-b");
    assert_eq!(entries[1].name, "dir-a");
    assert_eq!(entries[2].name, "large.txt");
    assert_eq!(entries[3].name, "small.txt");
}
