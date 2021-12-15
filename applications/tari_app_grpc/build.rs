fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .format(false)
        .compile(
            &[
                "proto/base_node.proto",
                "proto/wallet.proto",
                "proto/validator_node.proto",
            ],
            &["proto"],
        )?;

    Ok(())
}
