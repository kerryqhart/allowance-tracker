//! Builds the pinned lgs and places the binary where bundling picks it up.
fn main() {
    println!("cargo:rerun-if-env-changed=LGS_BINARY");
    // Unset: do nothing, and don't warn — an ordinary `cargo build` for
    // someone who just wants to run the app must keep working without lgs.
    let Ok(path) = std::env::var("LGS_BINARY") else {
        return;
    };
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap())
        .ancestors().nth(3).unwrap().join("lgs");
    // Set but wrong: fail loudly. A release build that silently omits the
    // bundled binary is worse than one that stops — the failure would
    // otherwise surface much later as a confusing runtime "binary not
    // found" instead of here, with the source path in hand.
    if let Err(err) = std::fs::copy(&path, &out) {
        println!("cargo:warning=LGS_BINARY={path} could not be copied to {out:?}: {err}");
        panic!("failed to copy LGS_BINARY={path} to {out:?}: {err}");
    }
}
