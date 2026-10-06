use pbs::adapters::adkernel_adn::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adkernelAdn", &Adapter::new("https://pbs2.adksrv.com/rtbpub?account={{.PublisherID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
