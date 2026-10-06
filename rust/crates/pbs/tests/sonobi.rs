use pbs::adapters::sonobi::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://apex.go.sonobi.com/prebid?partnerid=71d9d3d8af").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/sonobi", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
