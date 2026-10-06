use pbs::adapters::videobyte::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://mock.videobyte.com");
    let failures = run_json_bidder_test("tests/fixtures/videobyte", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
