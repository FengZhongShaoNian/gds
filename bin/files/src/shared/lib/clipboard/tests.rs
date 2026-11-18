use super::{Identity, Mode, Snapshot, fallback::TextCodec, state::Cache};
use std::{path::PathBuf, sync::Arc};

#[test]
fn stale_cut_validation_cannot_override_a_newer_intent() {
    let a = PathBuf::from("/tmp/stale-a");
    let b = PathBuf::from("/tmp/stale-b");
    let cut = Snapshot::new(Mode::Cut, vec![a.clone()]).expect("cut snapshot");
    let mut cache = Cache::default();
    cache.record_write(&cut);
    let generation = cache.begin_refresh().expect("validation read");
    assert!(
        cache.begin_refresh().is_none(),
        "only one validation read is in flight"
    );
    let newer = Snapshot::new(Mode::Cut, vec![b.clone()]).expect("newer cut");
    cache.record_write(&newer);
    let validating_generation = cache
        .begin_refresh()
        .expect("the newer intent validates at once");

    assert!(!cache.reconcile(generation, Ok(Some(cut)), false));
    assert!(cache.is_cut(&b) && !cache.is_cut(&a));
    assert!(
        cache.begin_refresh().is_none(),
        "a stale completion must not release the newer read"
    );
    assert!(!cache.reconcile(validating_generation, Ok(Some(newer)), false));
    assert!(cache.is_cut(&b));
}

#[test]
fn shared_snapshot_removal_preserves_order_and_rejects_replaced_identity() {
    let paths: Vec<PathBuf> = (0..512)
        .map(|index| PathBuf::from(format!("/tmp/clip-{index}")))
        .collect();
    let removed = PathBuf::from("/tmp/clip-257");
    let expected = Arc::new(Snapshot::new(Mode::Copy, paths.clone()).expect("copy snapshot"));
    let current = expected.as_ref().clone();

    let remaining = expected
        .excluding(&current, std::slice::from_ref(&removed))
        .expect("matching snapshot removes target entry");
    assert_eq!(remaining.len(), paths.len() - 1);
    assert!(!remaining.contains(&removed));
    assert_eq!(remaining[256], PathBuf::from("/tmp/clip-256"));
    assert_eq!(remaining[257], PathBuf::from("/tmp/clip-258"));
    assert!(
        expected
            .excluding(&current, &[PathBuf::from("/tmp/not-in-clipboard")])
            .is_none(),
        "absent paths must not republish an unchanged offer"
    );

    let replaced = Snapshot::new(Mode::Copy, paths.clone()).expect("newer copy with same paths");
    assert!(
        expected
            .excluding(&replaced, std::slice::from_ref(&removed))
            .is_none(),
        "a newer offer with the same paths has a distinct identity and must not be overwritten"
    );

    let foreign = Snapshot {
        mode: Mode::Copy,
        paths,
        identity: Identity(None),
    };
    assert!(
        expected
            .excluding(&foreign, std::slice::from_ref(&removed))
            .is_none(),
        "a foreign clipboard offer must not be overwritten"
    );
}

#[test]
fn text_fallback_round_trips_tagged_snapshots_and_ignores_ordinary_text() {
    let snapshot = Snapshot::new(
        Mode::Cut,
        vec![PathBuf::from("/tmp/alpha"), PathBuf::from("/tmp/beta")],
    )
    .expect("valid snapshot");
    let encoded = TextCodec::encode_snapshot(&snapshot).expect("encoded fallback text");
    assert!(encoded.starts_with("# gds-files-v1:"));
    let decoded = TextCodec::decode_snapshot(Some(&encoded))
        .expect("decode should succeed")
        .expect("tagged payload should decode");
    assert_eq!(decoded, snapshot);

    assert!(
        TextCodec::decode_snapshot(Some("file:///tmp/plain-text-not-gds\r\n"))
            .expect("plain text URI list must not error")
            .is_none(),
        "ordinary clipboard text must never be treated as copied files"
    );
    assert!(
        TextCodec::decode_uris(Some("https://example.com/file"))
            .expect("plain URL text must not error")
            .is_none()
    );
}
