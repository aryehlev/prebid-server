use pbs::adapters::roulax::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/roulax", &Adapter::new("http://dsp.rcoreads.com/api/vidmate?pid=vidmate_android_banner").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
