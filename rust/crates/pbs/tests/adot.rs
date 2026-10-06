use pbs::adapters::adot::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://dsp.adotmob.com/headerbidding{PUBLISHER_PATH}/bidrequest");
    let failures = run_json_bidder_test("tests/fixtures/adot", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
