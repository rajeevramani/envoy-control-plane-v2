use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Generate Rust code from protobuf definitions
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .out_dir(&out_dir)
        .compile_protos(&["proto/envoy/service/discovery/v3/ads.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/");
    Ok(())
}
