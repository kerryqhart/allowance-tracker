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
            for needle in [
                "std::fs",
                "std::net",
                "std::env",
                "std::io",
                "std::process",
                "std::os",
                "File::open",
                "File::create",
                "Command::new",
                "include_str!",
                "include_bytes!",
            ] {
                if line.contains(needle) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, needle));
                }
            }
        }
    }
}

#[test]
fn crate_dependencies_whitelist_only() {
    let allowed_deps = ["serde", "serde_json", "chrono", "csv", "thiserror"];

    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();

    // Find the [dependencies] section
    let in_deps = manifest
        .lines()
        .skip_while(|line| !line.starts_with("[dependencies]"))
        .skip(1) // skip the [dependencies] header
        .take_while(|line| !line.starts_with("["));

    let mut found_deps = Vec::new();
    for line in in_deps {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("#") {
            continue;
        }
        // Extract the crate name (before = or space)
        if let Some(name) = trimmed.split(|c: char| c == '=' || c.is_whitespace()).next() {
            let name = name.trim();
            if !name.is_empty() {
                found_deps.push(name.to_string());
            }
        }
    }

    let mut offenders = Vec::new();
    for dep in &found_deps {
        if !allowed_deps.contains(&dep.as_str()) {
            offenders.push(dep.clone());
        }
    }

    assert!(
        offenders.is_empty(),
        "allowance-core dependencies must be whitelist-only (serde, serde_json, chrono, csv, thiserror). \
         Found disallowed: {:?}",
        offenders
    );
}
