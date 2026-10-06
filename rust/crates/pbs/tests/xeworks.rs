use pbs::adapters::xeworks::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://prebid-srv.xe.works/?pid={{.SourceId}}&host={{.Host}}").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/xeworks", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
