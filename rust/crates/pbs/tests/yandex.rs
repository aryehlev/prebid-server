use pbs::adapters::yandex::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/yandex", &Adapter::new("https://bs-metadsp.yandex.ru/prebid/{{.PageID}}?ssp-id=10500").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
