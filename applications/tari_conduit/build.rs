fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/base_node.proto");
    tonic_build::compile_protos("../tari_base_node/proto/base_node.proto")?;
    Ok(())
}
