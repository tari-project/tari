fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("../tari_app_grpc/proto/base_node.proto")?;
    Ok(())
}
