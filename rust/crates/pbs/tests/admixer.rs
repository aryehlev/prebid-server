use pbs::adapters::admixer::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/admixer", &Adapter::new("http://inv-nets.admixer.net/pbs.aspx"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
