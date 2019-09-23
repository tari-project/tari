#![feature(type_alias_impl_trait)]

use tari_comms::middleware::MiddlewareError;
use tower::{layer::Layer, Service, ServiceBuilder};

mod deserialize;
mod dht;
mod messages;
mod utils;

/// Returns an the full DHT stack as a `tower::layer::Layer`. This can be composed with
/// other inbound middleware services which expect an DhtInboundMessage
pub fn inbound_layer<S>() -> impl Layer<S>
where S: Service<messages::DhtInboundMessage, Response = (), Error = MiddlewareError> {
    ServiceBuilder::new()
        .layer(deserialize::DhtDeserializeLayer::new())
        .layer(dht::DhtLayer::new())
        .into_inner()
}
