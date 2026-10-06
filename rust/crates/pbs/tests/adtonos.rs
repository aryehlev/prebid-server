use pbs::adapters::adtonos::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://exchange.example.com/bid/{{.PublisherID}}").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/adtonos", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
