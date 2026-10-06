use pbs::adapters::nextmillennium::Adapter;
use pbs::config::Server;
use pbs::testing::run_json_bidder_test;

fn server() -> Server {
    Server { external_url: "http://hosturl.com".into(), gvl_id: 1, data_center: "2".into() }
}

#[test]
fn json_samples() {
    let bidder = Adapter::new("https://pbs.nextmillmedia.com/openrtb2/auction", "", &server()).expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/nextmillennium", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn builder_with_extra_info_reads_flags() {
    let bidder = Adapter::new(
        "https://pbs.nextmillmedia.com/openrtb2/auction",
        "{\"nmmFlags\":[\"flag1\",\"flag2\"]}",
        &server(),
    );
    assert!(bidder.is_ok());
    assert!(Adapter::new("x", "not json", &server()).is_err());
}
