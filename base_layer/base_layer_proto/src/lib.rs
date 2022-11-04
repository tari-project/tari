#[allow(clippy::large_enum_variant)]
pub mod base_node {
    include!(concat!(env!("OUT_DIR"), "/tari.base_node.rs"));
}

pub mod core {
    include!(concat!(env!("OUT_DIR"), "/tari.core.rs"));
}

pub mod mempool {
    include!(concat!(env!("OUT_DIR"), "/tari.mempool.rs"));
}

#[allow(clippy::large_enum_variant)]
pub mod transaction_protocol {
    include!(concat!(env!("OUT_DIR"), "/tari.transaction_protocol.rs"));
}

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/tari.types.rs"));
}
