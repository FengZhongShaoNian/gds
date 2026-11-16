use std::collections::BTreeSet;

use super::{Catalog, FluentArgs, Language, resource_for};

fn extract_entries(ftl: &str) -> Vec<(&str, &str)> {
    ftl.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (key, value) = trimmed.split_once('=')?;
            Some((key.trim(), value.trim()))
        })
        .collect()
}

fn extract_variables(pattern: &str) -> BTreeSet<&str> {
    let mut vars = BTreeSet::new();
    let mut rest = pattern;
    while let Some(dollar) = rest.find('$') {
        let after_dollar = &rest[dollar + 1..];
        let end = after_dollar
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .unwrap_or(after_dollar.len());
        if end > 0 {
            vars.insert(&after_dollar[..end]);
        }
        rest = &after_dollar[end..];
    }
    vars
}

fn assert_all_entries_resolve(catalog: &Catalog, entries: &[(&str, &str)]) {
    for &(key, pattern) in entries {
        let vars = extract_variables(pattern);
        if vars.is_empty() {
            let value = catalog.text(key);
            assert!(
                value.is_some_and(|v| !v.trim().is_empty() && v != key),
                "missing or unresolved localization key: {key}"
            );
        } else {
            let mut args = FluentArgs::new();
            for var in vars {
                args.set(var, "sample");
            }
            let value = catalog.format(key, &args);
            assert!(
                value.is_some_and(|v| !v.trim().is_empty() && v != key),
                "missing or unresolved parameterized localization key: {key}"
            );
        }
    }
}

#[test]
fn english_and_chinese_catalogs_have_matching_non_empty_keys() {
    let en_src = resource_for(Language::English);
    let zh_src = resource_for(Language::Chinese);
    let en_entries = extract_entries(en_src);
    let zh_entries = extract_entries(zh_src);

    let en_keys: Vec<_> = en_entries.iter().map(|(k, _)| *k).collect();
    let zh_keys: Vec<_> = zh_entries.iter().map(|(k, _)| *k).collect();
    assert_eq!(en_keys, zh_keys, "en-US and zh-CN FTL keys diverged");

    for (&(key, en_pattern), &(_, zh_pattern)) in en_entries.iter().zip(zh_entries.iter()) {
        assert_eq!(
            extract_variables(en_pattern),
            extract_variables(zh_pattern),
            "placeholder variables diverged for key: {key}"
        );
    }

    let en_catalog = Catalog::new(Language::English, &[en_src]).expect("parse en-US FTL");
    let zh_catalog = Catalog::new(Language::Chinese, &[zh_src]).expect("parse zh-CN FTL");
    assert_all_entries_resolve(&en_catalog, &en_entries);
    assert_all_entries_resolve(&zh_catalog, &zh_entries);
}
