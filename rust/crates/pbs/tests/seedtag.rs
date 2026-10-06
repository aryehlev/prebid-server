use pbs::adapters::seedtag::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://url.seedtag.com").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/seedtag", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
