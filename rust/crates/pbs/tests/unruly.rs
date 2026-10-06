use pbs::adapters::unruly::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://targeting.unrulymedia.com/unruly_prebid_server").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/unruly", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
