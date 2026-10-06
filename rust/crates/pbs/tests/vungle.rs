use pbs::adapters::vungle::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/vungle", &Adapter::new("https://vungle.io/bit/t"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
