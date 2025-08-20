use crate::stratum::{
    nicehash_stream_adapter::NiceHashStyleStatumStreamAdapter,
    stratum_v1_adapter::StratumV1StreamAdapter,
    stream_adapter::StratumStreamAdapter,
    StratumRequest,
};

pub(crate) struct MultiVersionStratumStreamAdapter {}

impl StratumStreamAdapter for MultiVersionStratumStreamAdapter {
    fn try_convert(line: String) -> anyhow::Result<StratumRequest> {
        dbg!("converting line: {}", &line);
        // Try NiceHash style first
        if let Ok(request) = NiceHashStyleStatumStreamAdapter::try_convert(line.clone()) {
            Ok(request)
        } else {
            // Fallback to Stratum V1 style
            StratumV1StreamAdapter::try_convert(line)
        }
    }
}
