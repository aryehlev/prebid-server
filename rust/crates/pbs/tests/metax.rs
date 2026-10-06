use pbs::adapters::metax::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://hb.metaxads.com/prebid?sid={{.PublisherID}}&adunit={{.AdUnit}}&source=prebid-server").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/metax", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
