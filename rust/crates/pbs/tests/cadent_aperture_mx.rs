use pbs::adapters::cadent_aperture_mx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let mut adapter = Adapter::new("https://hb.emxdgt.com");
    adapter.set_testing(true);
    let failures = run_json_bidder_test("tests/fixtures/cadent_aperture_mx", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
