fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Safety: set_var is used in a single-threaded build script environment.
    unsafe {
        std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    }

    tonic_build::configure().compile(&["proto/nexus.proto"], &["proto"])?;
    Ok(())
}
