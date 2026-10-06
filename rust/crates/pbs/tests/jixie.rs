use pbs::adapters::jixie::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/jixie", &Adapter::new("https://hb.jixie.io/v2/hbsvrpost"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
