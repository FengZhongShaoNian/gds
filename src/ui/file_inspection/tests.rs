use super::{Cache, DEFAULT_CACHE_ENTRIES, DEFAULT_CACHE_MIB, DEFAULT_WORKERS, Options};

#[test]
fn inspection_defaults_match_specification() {
    let options = Options::default();
    assert_eq!(options.capacity, DEFAULT_CACHE_ENTRIES);
    assert_eq!(options.byte_budget, DEFAULT_CACHE_MIB * 1024 * 1024);
    assert_eq!(options.workers, DEFAULT_WORKERS);
    let cache = Cache::default();
    assert_eq!(cache.options(), options);
}
