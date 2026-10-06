use pbs::adapters::triplelift_native::Adapter;
use pbs::testing::run_json_bidder_test;

const ENDPOINT: &str = "http://tlx.3lift.net/s2sn/auction?supplier_id=20";

#[test]
fn json_samples() {
    let bidder = Adapter::new(ENDPOINT, r#"{"publisher_whitelist":["foo","bar","baz"]}"#).unwrap();
    let failures = run_json_bidder_test("tests/fixtures/triplelift_native", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn bad_config() {
    assert!(Adapter::new(ENDPOINT, "{foo:2}").is_err());
}

#[test]
fn empty_config() {
    assert!(Adapter::new(ENDPOINT, "").is_ok());
}
