use pbs::adapters::beachfront::Adapter;
use pbs::testing::run_json_bidder_test;

const DEFAULT_VIDEO_ENDPOINT: &str = "https://reachms.bfmio.com/bid.json?exchange_id";

#[test]
fn json_samples() {
    let adapter = Adapter::new(
        "https://qa.beachrtb.com/prebid_display",
        r#"{"video_endpoint":"https://qa.beachrtb.com/bid.json?exchange_id"}"#,
    )
    .unwrap();
    let failures = run_json_bidder_test("tests/fixtures/beachfront", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn extra_info_default_when_empty() {
    let a = Adapter::new("https://qa.beachrtb.com/prebid_display", "").unwrap();
    assert_eq!(a.video_endpoint(), DEFAULT_VIDEO_ENDPOINT);
}

#[test]
fn extra_info_default_when_not_specified() {
    let a = Adapter::new("https://qa.beachrtb.com/prebid_display", r#"{"video_endpoint":""}"#).unwrap();
    assert_eq!(a.video_endpoint(), DEFAULT_VIDEO_ENDPOINT);
}

#[test]
fn extra_info_malformed() {
    assert!(Adapter::new("https://qa.beachrtb.com/prebid_display", "malformed").is_err());
}
