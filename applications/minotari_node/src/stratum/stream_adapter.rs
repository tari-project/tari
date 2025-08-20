use crate::stratum::StratumRequest;

pub trait StratumStreamAdapter {
    fn try_convert(line: String) -> anyhow::Result<StratumRequest>;
}
