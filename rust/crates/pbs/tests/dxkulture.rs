use pbs::adapters::dxkulture::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/dxkulture", &Adapter::new("https://ads.dxkulture.com/pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
