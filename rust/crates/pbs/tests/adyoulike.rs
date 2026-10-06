use pbs::adapters::adyoulike::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://localhost/bid/4").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/adyoulike", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
