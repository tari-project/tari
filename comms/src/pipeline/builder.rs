// Copyright 2020, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{message::InboundMessage, outbound_message_service::OutboundMessage, pipeline::SinkService};
use derive_error::Error;
use futures::channel::mpsc;
use tower::Service;

const DEFAULT_MAX_CONCURRENT_TASKS: usize = 50;
const DEFAULT_OUTBOUND_BUFFER_SIZE: usize = 50;

type MpscSinkService = SinkService<mpsc::Sender<OutboundMessage>>;

#[derive(Default)]
pub struct Builder<TInSvc, TOutSvc, TOutReq> {
    max_concurrent_inbound_tasks: usize,
    outbound_buffer_size: usize,
    inbound: Option<TInSvc>,
    outbound_rx: Option<mpsc::Receiver<TOutReq>>,
    outbound_pipeline_factory: Option<Box<dyn FnOnce(MpscSinkService) -> TOutSvc>>,
}

impl Builder<(), (), ()> {
    pub fn new() -> Self {
        Self {
            max_concurrent_inbound_tasks: DEFAULT_MAX_CONCURRENT_TASKS,
            outbound_buffer_size: DEFAULT_OUTBOUND_BUFFER_SIZE,
            inbound: None,
            outbound_rx: None,
            outbound_pipeline_factory: None,
        }
    }
}

impl<TInSvc, TOutSvc, TOutReq> Builder<TInSvc, TOutSvc, TOutReq> {
    pub fn max_concurrent_inbound_tasks(mut self, max_tasks: usize) -> Self {
        self.max_concurrent_inbound_tasks = max_tasks;
        self
    }

    pub fn outbound_buffer_size(mut self, buf_size: usize) -> Self {
        self.outbound_buffer_size = buf_size;
        self
    }

    pub fn with_outbound_pipeline<F, S, R>(self, receiver: mpsc::Receiver<R>, factory: F) -> Builder<TInSvc, S, R>
    where
        // Factory function takes in a SinkService and returns a new composed service
        F: FnOnce(MpscSinkService) -> S + 'static,
        S: Service<R> + Clone + Send + 'static,
    {
        Builder {
            outbound_rx: Some(receiver),
            outbound_pipeline_factory: Some(Box::new(factory)),

            max_concurrent_inbound_tasks: self.max_concurrent_inbound_tasks,
            inbound: self.inbound,
            outbound_buffer_size: self.outbound_buffer_size,
        }
    }

    pub fn with_inbound_pipeline<S>(self, inbound: S) -> Builder<S, TOutSvc, TOutReq>
    where S: Service<InboundMessage> + Clone + Send + 'static {
        Builder {
            inbound: Some(inbound),

            max_concurrent_inbound_tasks: self.max_concurrent_inbound_tasks,
            outbound_rx: self.outbound_rx,
            outbound_pipeline_factory: self.outbound_pipeline_factory,
            outbound_buffer_size: self.outbound_buffer_size,
        }
    }
}

impl<TInSvc, TOutSvc, TOutReq> Builder<TInSvc, TOutSvc, TOutReq>
where
    TOutSvc: Service<TOutReq> + Clone + Send + 'static,
    TInSvc: Service<InboundMessage> + Clone + Send + 'static,
{
    fn build_outbound(
        &mut self,
    ) -> Result<OutboundPipelineConfig<mpsc::Receiver<TOutReq>, TOutSvc>, PipelineBuilderError> {
        let (out_sender, out_receiver) = mpsc::channel(self.outbound_buffer_size);

        let in_receiver = self
            .outbound_rx
            .take()
            .ok_or_else(|| PipelineBuilderError::OutboundPipelineNotProvided)?;
        let factory = self
            .outbound_pipeline_factory
            .take()
            .ok_or_else(|| PipelineBuilderError::OutboundPipelineNotProvided)?;
        let sink_service = SinkService::new(out_sender);
        let pipeline = (factory)(sink_service);
        Ok(OutboundPipelineConfig {
            in_receiver,
            pipeline,
            out_receiver,
        })
    }

    pub fn try_finish(mut self) -> Result<Config<TInSvc, TOutSvc, TOutReq>, PipelineBuilderError> {
        let inbound = self
            .inbound
            .take()
            .ok_or_else(|| PipelineBuilderError::InboundNotProvided)?;
        let outbound = self.build_outbound()?;

        Ok(Config {
            max_concurrent_inbound_tasks: self.max_concurrent_inbound_tasks,
            inbound,
            outbound,
        })
    }

    pub fn finish(self) -> Config<TInSvc, TOutSvc, TOutReq> {
        self.try_finish().unwrap()
    }
}

pub struct OutboundPipelineConfig<TInStream, TPipeline> {
    /// Messages read from this stream are passed to the pipeline
    pub in_receiver: TInStream,
    /// Receiver of `OutboundMessage`s coming from the pipeline
    pub out_receiver: mpsc::Receiver<OutboundMessage>,
    /// The pipeline (`tower::Service`) to run for each in_stream message
    pub pipeline: TPipeline,
}

pub struct Config<TInSvc, TOutSvc, TOutReq> {
    pub max_concurrent_inbound_tasks: usize,
    pub inbound: TInSvc,
    pub outbound: OutboundPipelineConfig<mpsc::Receiver<TOutReq>, TOutSvc>,
}

#[derive(Debug, Error)]
pub enum PipelineBuilderError {
    /// Inbound pipeline was not provided
    InboundNotProvided,
    /// Outbound pipeline was not provided
    OutboundPipelineNotProvided,
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::future;
    use std::convert::identity;
    use tower::service_fn;

    #[test]
    fn minimal_usage() {
        // Called when a message is sent on the given channel.
        let (_, rx) = mpsc::channel::<OutboundMessage>(1);

        let config = Builder::new()
            .max_concurrent_inbound_tasks(50)
            // Forward all messages on rx_out to the provided SinkService
            .with_outbound_pipeline(rx, identity)
            // Discard all inbound messages
            .with_inbound_pipeline(service_fn(|_| future::ready(Result::<_, ()>::Ok(()))))
            .finish();

        assert_eq!(config.max_concurrent_inbound_tasks, 50);
    }
}
