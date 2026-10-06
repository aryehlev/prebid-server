use pbs::adapters::gumgum::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/gumgum", &Adapter::new("https://g2.gumgum.com/providers/prbds2s/bid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
