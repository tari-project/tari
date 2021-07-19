use jsonrpc_http_server::{
    jsonrpc_core::{IoHandler, Params, Value},
    ServerBuilder,
};

use tari_app_grpc::tari_rpc as grpc;

fn main() {
    let mut io = IoHandler::default();
    io.add_method("say_hello", |_params: Params| async {
        dbg!("Hello called");
        Ok(Value::String("hello".to_owned()))
    });

    io.add_method("eth_accounts", |_params: Params| async {
        dbg!("eth_accounts called");

        let accounts = vec!["0x407d73d8a49eeb85d32cf465507dd71d507100c1"];
        Ok(Value::Array(
            accounts.into_iter().map(|s| Value::String(s.to_string())).collect(),
        ))
    });

    let server = ServerBuilder::new(io)
        .threads(3)
        .start_http(&"127.0.0.1:3030".parse().unwrap())
        .unwrap();

    server.wait();
}

async fn get_wallet_accounts() {
    let wallet_client = grpc::wallet_client::WalletClient::connect(format!("http://{}", "127.0.0.1:18144")).await?;
    wallet_client.
}
