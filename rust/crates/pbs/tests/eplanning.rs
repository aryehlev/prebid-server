use pbs::adapters::eplanning::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let mut adapter = Adapter::new("http://rtb.e-planning.net/pbs/1").expect("builder");
    adapter.set_testing(true);
    let failures = run_json_bidder_test("tests/fixtures/eplanning", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
