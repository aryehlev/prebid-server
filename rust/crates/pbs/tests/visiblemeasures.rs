use pbs::adapters::visiblemeasures::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://example.com/pserver").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/visiblemeasures", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
