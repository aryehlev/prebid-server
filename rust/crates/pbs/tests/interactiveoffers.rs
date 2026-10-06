use pbs::adapters::interactiveoffers::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/interactiveoffers", &Adapter::new("https://prebid-server.ioadx.com/bidRequest/?partnerId=d9e56d418c4825d466ee96c7a31bf1da6b62fa04").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
