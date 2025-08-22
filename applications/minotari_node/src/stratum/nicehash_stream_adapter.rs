use crate::stratum::{stream_adapter::StratumStreamAdapter, StratumRequest};

pub struct NiceHashStyleStatumStreamAdapter {}

impl StratumStreamAdapter for NiceHashStyleStatumStreamAdapter {
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
            "login" => {
                let params = json["params"]
                    .as_object()
                    .ok_or(anyhow::anyhow!("Invalid JSON.params missing"))?;
                let login = params["login"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. login missing"))?
                    .to_string();
                let (address, worker) = if login.contains(".") {
                    let mut parts = login.splitn(2, ".");
                    let address = parts.next().unwrap_or("").to_string();
                    let worker = parts.next().map(|s| s.to_string());
                    (address, worker)
                } else {
                    (login.clone(), None)
                };
                let pass = params["pass"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. pass missing"))?
                    .to_string();
                let agent = params["agent"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. agent missing"))?
                    .to_string();
                let algo = if let Some(a) = params.get("algo") {
                    a.as_array()
                        .ok_or(anyhow::anyhow!("Invalid JSON. algo missing"))?
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                Ok(StratumRequest::Login {
                    id,
                    login,
                    address,
                    worker,
                    pass,
                    agent,
                    algo,
                })
            },
            "submit" => {
                let params = json["params"].as_object().ok_or(anyhow::anyhow!("Invalid JSON"))?;
                let job_id = params["job_id"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. job_id missing"))?
                    .to_string();
                let nonce = params["nonce"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. nonce missing"))?
                    .to_string();
                let result = params["result"]
                    .as_str()
                    .ok_or(anyhow::anyhow!("Invalid JSON. result missing"))?
                    .to_string();
                let pow = if let Some(p) = params.get("pow") {
                    Some(
                        p.as_array()
                            .ok_or(anyhow::anyhow!("Invalid JSON. pow missing"))?
                            .iter()
                            .filter_map(|v| v.as_u64())
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                };
                Ok(StratumRequest::Submit {
                    id,
                    job_id,
                    nonce,
                    result,
                    pow,
                })
            },
            _ => Err(anyhow::anyhow!("Unknown method")),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_login_adapt() {
        let json = r#"{"id":1,"jsonrpc":"2.0","method":"login","params":{"agent":"lolMiner 1.97","login":"12ieBKPoibbttq2czfvy9wWBwnKw6yZqHjYv71pY8GhJHYSVUUVBs8AwLnDx6s6RZVTk98zoVrRnMhsy1XrSErAAe3.lolMiner","pass":"x"}}"#;
        let request = NiceHashStyleStatumStreamAdapter::try_convert(json.to_string()).unwrap();
        match request {
            StratumRequest::Login {
                id,
                login,
                address,
                worker,
                pass,
                agent,
                algo,
            } => {
                assert_eq!(id, "1");
                assert_eq!(
                    login,
                    "12ieBKPoibbttq2czfvy9wWBwnKw6yZqHjYv71pY8GhJHYSVUUVBs8AwLnDx6s6RZVTk98zoVrRnMhsy1XrSErAAe3.\
                     lolMiner"
                );
                assert_eq!(
                    address,
                    "12ieBKPoibbttq2czfvy9wWBwnKw6yZqHjYv71pY8GhJHYSVUUVBs8AwLnDx6s6RZVTk98zoVrRnMhsy1XrSErAAe3"
                );
                assert_eq!(worker, Some("lolMiner".to_string()));
                assert_eq!(pass, "x");
                assert_eq!(agent, "lolMiner 1.97");
                assert!(algo.is_empty());
            },
            _ => panic!("Expected login request"),
        }
    }
}
