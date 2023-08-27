// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use proc_macro::TokenStream;

#[macro_use]
mod macros;

mod expand;
mod generator;
mod method_info;
mod options;

/// #[taiji_rpc(...)] proc macro attribute
///
/// Generates Taiji RPC "harness code" for a given trait.
///
/// ```no_run,ignore
/// # use taiji_comms_rpc_macros::taiji_rpc;
/// # use taiji_comms::protocol::rpc::{Request, Streaming, Response, RpcStatus, RpcServer};
/// use taiji_comms::{framing, memsocket::MemorySocket};
///
/// #[taiji_rpc(protocol_name = b"/taiji/greeting/1.0", server_struct = GreetingServer, client_struct = GreetingClient)]
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
/// #[taiji_comms::async_trait]
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
/// `taiji_rpc` options
/// - `protocol_name` is the value used during protocol negotiation
/// - `server_struct` is the name of the "server" struct that is generated
/// - `client_struct` is the name of the client struct that is generated
///
/// `rpc` attribute
/// - `method` is a unique number that uniquely identifies each function within the service. Once a `method` is used it
///   should never be reused (think protobuf field numbers).
#[proc_macro_attribute]
pub fn taiji_rpc(attr: TokenStream, item: TokenStream) -> TokenStream {
    let options = syn::parse_macro_input!(attr as options::RpcTraitOptions);
    let target_trait = syn::parse_macro_input!(item as syn::ItemTrait);
    let code = expand::expand_trait(target_trait, options);
    let ts = quote::quote! { #code };
    ts.into()
}
