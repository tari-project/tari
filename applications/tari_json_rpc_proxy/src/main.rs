use jsonrpc_http_server::{
    jsonrpc_core::{IoHandler, Params, Value},
    ServerBuilder,
};

use tari_app_grpc::tari_rpc as grpc;
use tari_utilities::hex::Hex;

fn main() {
    let mut io = IoHandler::default();
    io.add_method("say_hello", |_params: Params| async {
        dbg!("Hello called");
        Ok(Value::String("hello".to_owned()))
    });

    io.add_method("eth_chainId", |_params: Params| async {
        dbg!("eth_chainId called");
        Ok(Value::String("0x88".to_owned()))
    });

    io.add_method("eth_blockNumber", |_params: Params| async {
        dbg!("eth_blockNumber called");
        Ok(Value::String("0x7777".to_owned()))
    });

    io.add_method("net_version", |_params: Params| async {
        dbg!("net_version called");
        Ok(Value::String("Test".to_owned()))
    });

    io.add_method("eth_call", |params: Params| async {
        dbg!("eth_call called");
        dbg!(&params);

        struct CallParams {
            data: String,
            to: String,
        }

        let call_data = match params {
            Params::Array(values) => {
                let v = values.first().unwrap();
                CallParams {
                    data: v["data"].as_str().unwrap().to_string(),
                    to: v["to"].as_str().unwrap().to_string(),
                }
            },
            _ => return Ok(Value::String("Unexpected".to_owned())),
        };

        match &call_data.data.as_str()[0..10] {
            "0x313ce567" => {
                //  decimals
                Ok(Value::String("0x00".to_owned()))
            },
            "0x95d89b41" =>
            // symbol
            {
                Ok(Value::String("TXTR2".to_owned()))
            },
            "0x70a08231" =>
            // balance
            {
                Ok(Value::String("0x17".to_owned()))
            },
            _ => Ok(Value::String("don't know".to_owned())),
        }
    });

    io.add_method("eth_estimateGas", |_params: Params| async {
        dbg!("eth_estimateGas called");
        Ok(Value::String("0x1".to_owned()))
    });

    io.add_method("eth_gasPrice", |_params: Params| async {
        dbg!("eth_gasPrice called");
        Ok(Value::String("0x1".to_owned()))
    });

    io.add_method("eth_getTransactionCount", |_params: Params| async {
        dbg!("eth_getTransactionCount called");
        Ok(Value::String("0x00".to_owned()))
    });

    io.add_method("eth_sendRawTransaction", |params: Params| async {
        dbg!("eth_sendRawTransaction called");
        Ok(Value::String("not yet impl".to_owned()))
    });

    io.add_method("eth_getBalance", |params: Params| async {
        dbg!("eth_getBalance called");
        dbg!(params);
        Ok(Value::String("0x00".to_owned()))
    });

    io.add_method("eth_accounts", |_params: Params| async {
        dbg!("eth_accounts called");

        let accounts = get_wallet_accounts().await.unwrap();
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

async fn get_wallet_accounts() -> Result<Vec<String>, String> {
    let mut wallet_client = grpc::wallet_client::WalletClient::connect(format!("http://{}", "127.0.0.1:18144"))
        .await
        .unwrap();
    let owned = wallet_client
        .get_owned_tokens(grpc::GetOwnedTokensRequest {
            asset_public_key: Hex::from_hex("d458e49c9fe023e6706db830c7520ce520ffbbfe22edb26f5c3379d2d4a9b838")
                .unwrap(),
        })
        .await
        .unwrap()
        .into_inner();
    Ok(owned.tokens.into_iter().map(|o| o.unique_id.to_hex()).collect())
}
