use pbs::adapters::datablocks::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/datablocks", &Adapter::new("http://pbserver.dblks.net/openrtb2?sid={{.SourceId}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
