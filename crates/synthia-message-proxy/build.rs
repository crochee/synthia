fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .compile_protos(&["proto/message_proxy.proto"], &["proto/"])?;
    Ok(())
}
