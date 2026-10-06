use pbs::adapters::trustedstack::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    // Go test: Endpoint "https://example.trustedstack.com/rtb/prebid", Server.ExternalUrl "http://hosturl.com".
    let failures = run_json_bidder_test(
        "tests/fixtures/trustedstack",
        &Adapter::new("https://example.trustedstack.com/rtb/prebid", "http://hosturl.com"),
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
