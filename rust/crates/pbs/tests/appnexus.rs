use pbs::adapters::appnexus::Adapter;
use pbs::config;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let cfg = config::Adapter::with_endpoint("http://ib.adnxs.com/openrtb2");
    let bidder = Adapter::new(&cfg).expect("Builder").with_random_generator(Box::new(|| 10));
    let failures = run_json_bidder_test("tests/fixtures/appnexus", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
