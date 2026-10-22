fn main() {
    println!("cargo:rerun-if-changed=proto/muster.proto");
    println!("cargo:rerun-if-changed=proto/app.proto");
    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config.message_attribute(".", "#[serde(default, deny_unknown_fields)]");
    config.enum_attribute(".", "#[serde(rename_all = \"snake_case\")]");
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path().expect("vendored protoc"));
    config
        .compile_protos(&["proto/muster.proto", "proto/app.proto"], &["proto"])
        .expect("compile muster.proto and app.proto");
}
