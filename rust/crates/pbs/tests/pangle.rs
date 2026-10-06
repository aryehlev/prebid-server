use pbs::adapters::pangle::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://pangle.io/api/get_ads").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/pangle", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
