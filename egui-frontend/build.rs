//! Builds the pinned lgs and places the binary where bundling picks it up.
fn main() {
    println!("cargo:rerun-if-env-changed=LGS_BINARY");
    if let Ok(path) = std::env::var("LGS_BINARY") {
        let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap())
            .ancestors().nth(3).unwrap().join("lgs");
        let _ = std::fs::copy(&path, &out);
    }
}
