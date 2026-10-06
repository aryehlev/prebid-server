use pbs::adapters::aduptech::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://example.com/rtb/bid", r#"{"target_currency": "EUR"}"#).expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/aduptech", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn invalid_extra_adapter_info() {
    let err = Adapter::new("https://example.com/rtb/bid", r#"{"foo": "bar"}"#).err().expect("error");
    assert_eq!(err.to_string(), "invalid extra info: TargetCurrency is empty, pls check");
}

#[test]
fn invalid_target_currency() {
    let err = Adapter::new("https://example.com/rtb/bid", r#"{"target_currency": "INVALID"}"#).err().expect("error");
    assert_eq!(err.to_string(), "invalid extra info: invalid TargetCurrency INVALID, pls check");
}
