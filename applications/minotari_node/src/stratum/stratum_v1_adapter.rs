use crate::stratum::{stream_adapter::StratumStreamAdapter, StratumRequest};

pub(crate) struct StratumV1StreamAdapter {}

impl StratumStreamAdapter for StratumV1StreamAdapter {
    fn try_convert(line: String) -> anyhow::Result<StratumRequest> {
        let json: serde_json::Value = serde_json::from_str(&line)?;
        let method = json["method"]
            .as_str()
            .ok_or(anyhow::anyhow!("Json missing method field"))?;
        let id = json["id"]
            .as_i64()
            .ok_or(anyhow::anyhow!("Invalid JSON. Json missing id field"))?
            .to_string();
        match method {
            "mining.subscribe" => {
                dbg!("here");
                let params = json["params"]
                    .as_array()
                    .ok_or(anyhow::anyhow!("Invalid JSON.params missing"))?;
                let agent = params.get(0);
                let agent = agent.and_then(|v| v.as_str()).map(|s| s.to_string());

                // let address_and_worker = params
                //     .get(1)
                //     .and_then(|v| v.as_str())
                //     .ok_or(anyhow::anyhow!("Invalid JSON. address missing"))?
                //     .to_string();
                // let address_parts = address_and_worker.split('.').collect::<Vec<_>>();
                // let address = address_parts[0].to_string();
                // let worker = if address_parts.len() > 1 {
                //     Some(address_parts[1].to_string())
                // } else {
                //     None
                // };
                Ok(StratumRequest::Subscribe {
                    id,
                    agent: agent.unwrap_or_default(),
                    // address,
                    // worker,
                })
            },
            "mining.authorize" => {
                let params = json["params"]
                    .as_array()
                    .ok_or(anyhow::anyhow!("Invalid JSON.params missing"))?;
                let login = params
                    .get(0)
                    .and_then(|v| v.as_str())
                    .ok_or(anyhow::anyhow!("Invalid JSON. login missing"))?
                    .to_string();
                let (worker_name, mut login) = if login.contains(".") {
                    let parts: Vec<&str> = login.split('.').collect();
                    (Some(parts[1].to_string()), parts[0].to_string())
                } else {
                    (None, login)
                };
                let is_solo = if login.starts_with("solo:") {
                    login = login.replace("solo:", "");
                    true
                } else {
                    false
                };
                let pass = params
                    .get(1)
                    .and_then(|v| v.as_str())
                    .ok_or(anyhow::anyhow!("Invalid JSON. pass missing"))?
                    .to_string();
                Ok(StratumRequest::Authorize {
                    id,
                    login,
                    is_solo,
                    pass,
                    worker_name,
                })
            },
            "mining.extranonce.subscribe" => Ok(StratumRequest::ExtraNonceSubscribe { id }),

            _ => Err(anyhow::anyhow!("Unknown method")),
        }
    }
}
