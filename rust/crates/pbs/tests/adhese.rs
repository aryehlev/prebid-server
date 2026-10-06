use pbs::adapters::adhese::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adhese", &Adapter::new("https://{{.AccountID}}.foo.bar/").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
