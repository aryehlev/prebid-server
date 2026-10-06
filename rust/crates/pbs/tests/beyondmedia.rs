use pbs::adapters::beyondmedia::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/beyondmedia", &Adapter::new("http://backend.andbeyond.media/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
