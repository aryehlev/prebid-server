use pbs::adapters::epom::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/epom", &Adapter::new("https://an.epom.com/ortb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
