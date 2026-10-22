//! Generates the iTerm2 protobuf types from iTerm2's api.proto (vendored in
//! proto/, the one copy that stays after the Swift adapters go).
fn main() {
    let proto = "proto/api.proto";
    println!("cargo:rerun-if-changed={proto}");
    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path().expect("vendored protoc"));
    config
        .compile_protos(&[proto], &["proto"])
        .expect("compile iTerm2 api.proto");
}
