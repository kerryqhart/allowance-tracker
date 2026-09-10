//! Round-trips the real `transactions.csv` for a child, if one is pointed to
//! by the `REAL_CSV` environment variable, through the canonical codec.
//!
//! This is deliberately never run automatically and never committed with
//! real data attached: `REAL_CSV` is read at test time only, and the file it
//! names never enters the repo. It is the local, one-time way to learn
//! whether the new hard-error codec (no `Utc::now()` fallback, no
//! `chrono::Local` date-only resolution, no derived transaction type) can
//! actually read a real, years-old transaction history — the synthesized
//! fixture in `tests/fixtures/transactions_legacy_shapes.csv` proves the
//! *property* holds, but only real data can prove the *real file* parses.
//!
//! Run explicitly:
//!   REAL_CSV=/path/to/transactions.csv \
//!     cargo test -p allowance-tracker-egui --test codec_real_data -- --ignored --nocapture

#[test]
#[ignore]
fn real_transactions_csv_round_trips_byte_for_byte() {
    let path = std::env::var("REAL_CSV")
        .expect("set REAL_CSV to the path of a real transactions.csv to run this test");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {path}: {e}"));

    let parsed_once = allowance_core::codec::parse_transactions(&text)
        .unwrap_or_else(|e| panic!("real data at {path} did not parse: {e}"));
    println!("parsed {} rows from {path}", parsed_once.len());

    let rendered_once = allowance_core::codec::render_transactions(&parsed_once);

    let parsed_twice = allowance_core::codec::parse_transactions(&rendered_once)
        .expect("re-parsing our own rendered output must succeed");
    let rendered_twice = allowance_core::codec::render_transactions(&parsed_twice);

    assert_eq!(
        rendered_once, rendered_twice,
        "render(parse(x)) must be a fixed point over the real file"
    );
}
