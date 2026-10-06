use pbs::adapters::flipp::Adapter;
use pbs::testing::run_json_bidder_test;

const FAKE_UUID: &str = "30470a14-2949-4110-abce-b62d57304ad5";

#[test]
fn json_samples() {
    let bidder = Adapter::with_uuid_generator("http://example.com/pserver", || Ok(FAKE_UUID.to_string()));
    let failures = run_json_bidder_test("tests/fixtures/flipp", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
