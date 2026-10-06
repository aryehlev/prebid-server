use pbs::adapters::vrtcal::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://rtb.vrtcal.com/bidder_prebid.vap?ssp=1804").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/vrtcal", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
