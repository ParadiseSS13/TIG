pub fn protocol_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

tonic::include_proto!("tig");
