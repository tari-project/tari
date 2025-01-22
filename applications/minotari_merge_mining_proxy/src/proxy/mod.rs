//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

pub(crate) mod inner;
mod monerod_method;
pub(crate) mod service;
pub(crate) mod static_responses;
pub(crate) mod utils;

#[cfg(test)]
mod test {
    use std::time::Instant;

    use anyhow::{anyhow, Error};
    use chrono::{Local, Timelike};
    use reqwest::Client;
    use serde_json::json;

    use crate::proxy::{monerod_method::MonerodMethod, utils::convert_reqwest_response_to_hyper_json_response};

    async fn get_json_rpc(
        method: MonerodMethod,
        json_rpc_port: u16,
        hyper_json_response: bool,
    ) -> Result<String, Error> {
        match method {
            MonerodMethod::GetHeight => get_response(json_rpc_port, method).await,
            MonerodMethod::GetBlockTemplate |
            MonerodMethod::GetVersion |
            MonerodMethod::SubmitBlock |
            MonerodMethod::GetBlockHeaderByHash |
            MonerodMethod::GetLastBlockHeader => json_rpc_request(method, json_rpc_port, hyper_json_response).await,
            _ => Err(anyhow!("'{}' not supported", method)),
        }
    }

    async fn get_response(json_rpc_port: u16, method: MonerodMethod) -> Result<String, Error> {
        let full_address = format!("http://127.0.0.1:{}", json_rpc_port);
        match reqwest::get(format!("{}/{}", full_address, method))
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
        {
            Ok(val) => {
                println!("{}: {}", method, val);
                Ok(val.to_string())
            },
            Err(e) => Err(e.into()),
        }
    }

    async fn json_rpc_request(
        method: MonerodMethod,
        json_rpc_port: u16,
        hyper_json_response: bool,
    ) -> Result<String, Error> {
        let rpc_method = format!("{}", method);
        let request_body = match method {
            MonerodMethod::GetBlockTemplate => json!({
                "jsonrpc": "2.0",
                "id": "0",
                "method": rpc_method,
                "params": {
                    "wallet_address": "489r43gR8bDMJNBf4Q6sL9CNERvZQrTqjRCSESqgWQEWWq2UGAfj2voaw3zBtD7U8CQ391Nc1PDHUHiN85yhbZnCDasqzyX",
                }
            }),
            MonerodMethod::GetVersion => json!({
                "jsonrpc": "2.0",
                "id": "0",
                "method": rpc_method,
                "params": {}
            }),
            MonerodMethod::SubmitBlock => json!({
                "jsonrpc": "2.0",
                "id": "0",
                "method": rpc_method,
                "params": ["1010c9a5cdbc062c8c6fc9c6b0d01299a8b4dd023dc4ee28d7f890deeae012ec11b04f0e3a24c4dc1e180002a4b1cb0101ffe8b0cb0101e0dde1dabd110323fd3161ea3cca2ad8208e66705d4f0ca9de911e99300ac813142891f000ccd1074e0321008fbbaf1ee6c5337eb65241e3f079ddf141c9a916f4b9bf3270b19ea4902c976b01cdf43802b1e9465e4bb2a6c9fc6e281d7b4f2b1aa819acfd8e6133446c895a550208b2f23f2262ef83aa000483b08f79f3bcc0a58d4c303d67db5839505eed4ed7273f6e5fc331c991011772ac88c18fd7f1c9f19b041b128fe923d95c2aafb1dd106bc781a61a2bfa1c66cb8e6dab82d22909b40bda27ec0e96aa0c6c0012023d353f3be941eb6ef1793cad0c773379302ec1567eb4d688b4e66499eee1855ffd97b966eb929ed39c1d056c"]
            }),
            MonerodMethod::GetBlockHeaderByHash => json!({
                "jsonrpc": "2.0",
                "id": "0",
                "method": rpc_method,
                "params": {
                    "hash":"e22cf75f39ae720e8b71b3d120a5ac03f0db50bba6379e2850975b4859190bc5"
                }
            }),
            MonerodMethod::GetLastBlockHeader => json!({
                "jsonrpc": "2.0",
                "id": "0",
                "method": rpc_method,
                "params": {}
            }),
            _ => return Err(anyhow!("'{}' not supported", method)),
        };
        let rpc_url = format!("http://127.0.0.1:{}/json_rpc", json_rpc_port);

        // Create an HTTP client
        let client = Client::new();

        // Send the POST request
        let response = client.post(rpc_url).json(&request_body).send().await?;

        // Parse the response body
        if response.status().is_success() {
            if hyper_json_response {
                let hyper_json_response = convert_reqwest_response_to_hyper_json_response(response).await?;

                println!();
                println!("{} - response:   {:?}", method, hyper_json_response);
                println!("{} - status:     {:?}", method, hyper_json_response.status());
                println!("{} - version:    {:?}", method, hyper_json_response.version());
                println!("{} - headers:    {:?}", method, hyper_json_response.headers());
                println!("{} - extensions: {:?}", method, hyper_json_response.extensions());
                println!("{} - body:       {:?}", method, hyper_json_response.body());

                let response_json = hyper_json_response.body();
                if response_json.get("error").is_some() {
                    return Err(anyhow!("'{}' failed ({})", method, response_json));
                }
                if response_json.get("result").is_none() {
                    return Err(anyhow!("'{}' failed ({})", method, response_json));
                }

                Ok(response_json.to_string())
            } else {
                let response_text = response.text().await?;
                let response_json: serde_json::Value = serde_json::from_str(&response_text)?;
                if response_json.get("error").is_some() {
                    return Err(anyhow!("'{}' failed ({})", method, response_text));
                }
                if response_json.get("result").is_none() {
                    return Err(anyhow!("'{}' failed ({})", method, response_text));
                }
                Ok(response_text)
            }
        } else {
            Err(anyhow!("{} failed({})", method, response.status()))
        }
    }

    fn time_now() -> String {
        let now = Local::now();
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            now.hour(),
            now.minute(),
            now.second(),
            now.timestamp_subsec_millis()
        )
    }

    pub(crate) async fn inner_json_rpc(
        method: MonerodMethod,
        json_rpc_port: u16,
        responses: &mut Vec<String>,
        count: usize,
        hyper_json_response: bool,
    ) {
        let start = Instant::now();
        let response = get_json_rpc(method, json_rpc_port, hyper_json_response).await;
        match response {
            Ok(val) => {
                responses.push(format!(
                    "  {}: method: {}; time now: {}; duration: {:.2?}, response length: {}",
                    count,
                    method,
                    time_now(),
                    start.elapsed(),
                    val.len(),
                ));
            },
            Err(err) => {
                responses.push(format!(
                    "  {}: method: {}; time now: {}; duration: {:.2?}, response length: {}, Error: {}",
                    count,
                    method,
                    time_now(),
                    start.elapsed(),
                    err.to_string().len(),
                    err
                ));
            },
        }
    }

    // To execute this test a merge mining proxy must be running (just verify the port, default used), ideally when
    // RandomX mining with XMRig is taking place.
    #[tokio::test]
    #[ignore]
    async fn test_get_monerod_info() {
        let json_rpc_port = 18181;
        let tick = tokio::time::Duration::from_secs(2);
        let mut interval = tokio::time::interval(tick);
        let mut responses = Vec::with_capacity(50);
        for method in [
            MonerodMethod::GetHeight,
            MonerodMethod::GetVersion,
            MonerodMethod::GetBlockTemplate,
        ] {
            let mut count = 0;
            responses.push(format!("method: {}, tick: {:.2?}", method, tick));
            loop {
                interval.tick().await;
                count += 1;
                inner_json_rpc(method, json_rpc_port, &mut responses, count, false).await;
                if count >= 1 {
                    break;
                }
            }
        }
        for response in responses {
            println!("{}", response);
        }
    }

    // To execute this test a merge mining proxy must be running (just verify the port, default used), ideally when
    // RandomX mining with XMRig is taking place.
    #[tokio::test]
    #[ignore]
    async fn stress_test_get_monerod_info() {
        let json_rpc_port = 18081;
        let tick = tokio::time::Duration::from_millis(1000);
        let mut interval = tokio::time::interval(tick);
        let mut responses = Vec::with_capacity(3010);
        for method in [
            MonerodMethod::GetHeight,
            MonerodMethod::GetVersion,
            MonerodMethod::GetBlockTemplate,
        ] {
            let mut count = 0;
            responses.push(format!("method: {}, tick: {:.2?}", method, tick));
            loop {
                interval.tick().await;
                count += 1;
                inner_json_rpc(method, json_rpc_port, &mut responses, count, false).await;
                if count >= 500 {
                    break;
                }
            }
        }
        for response in responses {
            println!("{}", response);
        }
    }
}
