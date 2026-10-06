use pbs::adapters::bidmyadz::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/bidmyadz", &Adapter::new("http://endpoint.bidmyadz.com/c0f68227d14ed938c6c49f3967cbe9bc"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
