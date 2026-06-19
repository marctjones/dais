#[test]
fn live_protocol_conformance() {
    dais_conformance::run_from_env().expect("live protocol conformance failed");
}
