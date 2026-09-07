//! The merge is only trustworthy if it is a total function over data.
//! This test is the enforcement the spec promises.

use std::path::Path;

#[test]
fn crate_source_performs_no_io() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    visit(&src, &mut offenders);
    assert!(offenders.is_empty(), "allowance-core must not do I/O: {offenders:?}");
}

fn visit(dir: &Path, offenders: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, offenders);
            continue;
        }
        if path.extension().map(|e| e != "rs").unwrap_or(true) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for needle in ["std::fs", "std::net", "File::open", "File::create", "Command::new"] {
                if line.contains(needle) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, needle));
                }
            }
        }
    }
}
