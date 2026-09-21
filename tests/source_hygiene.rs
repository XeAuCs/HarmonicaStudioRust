//! Source hygiene tripwire: all Rust sources must be plain UTF-8 with no
//! replacement characters or private-use leftovers. A wrong-encoding
//! read/write roundtrip (e.g. ANSI instead of UTF-8) is the failure mode
//! this guards against; ASCII-only file so this test cannot itself rot.
use std::path::{Path, PathBuf};

fn collect_sources(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn rust_sources_have_no_encoding_damage_markers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    collect_sources(&root, &mut sources);
    assert!(!sources.is_empty(), "no sources found");
    let mut checked = 0;
    for path in &sources {
        let bytes = std::fs::read(path).unwrap();
        let text = String::from_utf8(bytes).unwrap_or_else(|_| panic!("not UTF-8: {path:?}"));
        assert!(
            !text.contains('\u{FFFD}'),
            "replacement character in {path:?}"
        );
        assert!(
            !text.chars().any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c)),
            "private-use character in {path:?}"
        );
        checked += 1;
    }
    assert!(checked >= 20, "too few sources checked: {checked}");
}
