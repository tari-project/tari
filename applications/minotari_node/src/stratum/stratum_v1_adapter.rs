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

                Ok(StratumRequest::Subscribe {
                    id,
                    agent: agent.unwrap_or_default(),
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
                let (worker_name, login) = if login.contains(".") {
                    let parts: Vec<&str> = login.split('.').collect();
                    (Some(parts[1].to_string()), parts[0].to_string())
                } else {
                    (None, login)
                };
                let pass = params
                    .get(1)
                    .and_then(|v| v.as_str())
                    .ok_or(anyhow::anyhow!("Invalid JSON. pass missing"))?
                    .to_string();
                Ok(StratumRequest::Authorize {
                    id,
                    login,
                    pass,
                    worker_name,
                })
            },
            "mining.extranonce.subscribe" => Ok(StratumRequest::ExtraNonceSubscribe { id }),

            _ => Err(anyhow::anyhow!("Unknown method")),
        }
    }
}
