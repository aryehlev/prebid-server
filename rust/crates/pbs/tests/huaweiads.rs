use pbs::adapters::huaweiads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/huaweiads", &Adapter::new("https://huaweiads.com/adxtest/", r#"{"pkgNameConvert":[{"convertedPkgName":"com.example.pkgname1","unconvertedPkgNames":["com.example.p1","com.example.p2"],"unconvertedPkgNameKeyWords":["p3","p4"],"unconvertedPkgNamePrefixs":["com.example1","com.example2"],"exceptionPkgNames":["com.example.p7","com.example.p8"]},{"convertedPkgName":"com.example.pkgname2","unconvertedPkgNames":["com.example.p9","com.example.p10"],"unconvertedPkgNameKeyWords":["p11","p12"],"unconvertedPkgNamePrefixs":["com.example3","com.example4"],"exceptionPkgNames":["com.example.p15","com.example3.unchanged"]}]}"#).unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
