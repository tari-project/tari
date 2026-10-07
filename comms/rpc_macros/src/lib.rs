// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use proc_macro::TokenStream;

#[macro_use]
mod macros;

mod decode_budget;
mod expand;
mod generator;
mod method_info;
mod options;

/// #[tari_rpc(...)] proc macro attribute
///
/// Generates Tari RPC "harness code" for a given trait.
///
/// ```no_run,ignore
/// # use tari_comms_rpc_macros::tari_rpc;
/// # use tari_comms::protocol::rpc::{Request, Streaming, Response, RpcStatus, RpcServer};
/// use tari_comms::{framing, memsocket::MemorySocket};
///
/// #[tari_rpc(protocol_name = b"/tari/greeting/1.0", server_struct = GreetingServer, client_struct = GreetingClient)]
/// pub trait GreetingRpc: Send + Sync + 'static {
///     #[rpc(method = 1)]
///     async fn say_hello(&self, request: Request<String>) -> Result<Response<String>, RpcStatus>;
///     #[rpc(method = 2)]
///     async fn return_error(&self, request: Request<()>) -> Result<Response<()>, RpcStatus>;
///     #[rpc(method = 3)]
///     async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus>;
/// }
///
/// // GreetingServer and GreetingClient can be used
/// struct GreetingService;
/// #[tari_comms::async_trait]
/// impl GreetingRpc for GreetingService {
///     async fn say_hello(&self, request: Request<String>) -> Result<Response<String>, RpcStatus> {
///         unimplemented!()
///     }
///
///     async fn return_error(&self, request: Request<()>) -> Result<Response<()>, RpcStatus> {
///         unimplemented!()
///     }
///
///     async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus> {
///         unimplemented!()
///     }
/// }
///
/// fn server() {
///     let greeting = GreetingServer::new(GreetingService);
///     let server = RpcServer::new().add_service(greeting);
///     // CommsBuilder::new().add_rpc(server)
/// }
///
/// async fn client() {
///     // Typically you would obtain the client using `PeerConnection::connect_rpc`
///     let (socket, _) = MemorySocket::new_pair();
///     let mut client = GreetingClient::connect(framing::canonical(socket, 1024)).await.unwrap();
///     let _ = client.say_hello("Barnaby Jones".to_string()).await.unwrap();
/// }
/// ```
///
/// `tari_rpc` options
/// - `protocol_name` (required) is the value used during protocol negotiation
/// - `server_struct` (required) is the name of the "server" struct that is generated
/// - `client_struct` (required) is the name of the client struct that is generated
/// - `reserved_methods = [N, ...]` (optional) lists method numbers that must never be used again, e.g. those of removed
///   methods. Using one is a compile error.
///
/// `rpc` attribute
/// - `method` (required) is a unique, non-zero number that identifies each function within the service. Once a `method`
///   is used it should never be reused (think protobuf field numbers).
/// - `max_items` (optional) is the decode budget for the method: the maximum number of embedded message instances its
///   request, its response or each item of its response stream may carry (see `tari_comms::decode_budget`). Request and
///   response types must implement `DecodeBudget` (derive it with `#[derive(DecodeBudget)]`). The server checks
///   requests against it before decoding them and the generated client checks responses. Defaults to
///   `tari_comms::decode_budget::DEFAULT_MAX_DECODE_ITEMS`.
///
/// Every RPC method must have the form `async fn name(&self, request: Request<T>) -> Result<Response<U>, RpcStatus>`
/// or `async fn name(&self, request: Request<T>) -> Result<Streaming<U>, RpcStatus>`. Anything else is a compile error.
#[proc_macro_attribute]
pub fn tari_rpc(attr: TokenStream, item: TokenStream) -> TokenStream {
    let options = syn::parse_macro_input!(attr as options::RpcTraitOptions);
    let target_trait = syn::parse_macro_input!(item as syn::ItemTrait);
    expand::expand_trait(target_trait, options).into()
}

/// `#[derive(DecodeBudget)]` implements `tari_comms::decode_budget::DecodeBudget` for a prost message, oneof or enum.
///
/// It reads the `#[prost(...)]` field attributes (as emitted by prost-build) and generates a walker over the encoded
/// message that charges one instance for each message-typed field (optional, repeated, map values and oneof variants)
/// and descends into it with that type's own walker. `bytes`, `string` and scalar fields are never entered.
/// `tari_common::build::ProtobufCompiler` adds this derive to every tari protobuf type.
#[proc_macro_derive(DecodeBudget, attributes(prost))]
pub fn derive_decode_budget(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    decode_budget::expand(&input).into()
}
