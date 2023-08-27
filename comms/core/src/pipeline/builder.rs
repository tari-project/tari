// Copyright 2020, The Taiji Project
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

use thiserror::Error;
use tokio::sync::mpsc;
use tower::Service;

use crate::{
    message::{InboundMessage, OutboundMessage},
    pipeline::SinkService,
};

const DEFAULT_MAX_CONCURRENT_TASKS: usize = 50;

type OutboundMessageSinkService = SinkService<mpsc::UnboundedSender<OutboundMessage>>;

/// Message pipeline builder
#[derive(Default)]
pub struct Builder<TInSvc, TOutSvc, TOutReq> {
    max_concurrent_inbound_tasks: usize,
    max_concurrent_outbound_tasks: Option<usize>,
    inbound: Option<TInSvc>,
    outbound_rx: Option<mpsc::Receiver<TOutReq>>,
    outbound_pipeline_factory: Option<Box<dyn FnOnce(OutboundMessageSinkService) -> TOutSvc>>,
}

impl Builder<(), (), ()> {
    pub fn new() -> Self {
        Self {
            max_concurrent_inbound_tasks: DEFAULT_MAX_CONCURRENT_TASKS,
            max_concurrent_outbound_tasks: None,
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

    pub fn max_concurrent_outbound_tasks(mut self, max_tasks: usize) -> Self {
        self.max_concurrent_outbound_tasks = Some(max_tasks);
        self
    }

    pub fn with_outbound_pipeline<F, S, R>(self, receiver: mpsc::Receiver<R>, factory: F) -> Builder<TInSvc, S, R>
    where
        // Factory function takes in a SinkService and returns a new composed service
        F: FnOnce(OutboundMessageSinkService) -> S + 'static,
        S: Service<R> + Clone + Send + 'static,
    {
        Builder {
            outbound_rx: Some(receiver),
            outbound_pipeline_factory: Some(Box::new(factory)),

            max_concurrent_inbound_tasks: self.max_concurrent_inbound_tasks,
            max_concurrent_outbound_tasks: self.max_concurrent_outbound_tasks,
            inbound: self.inbound,
        }
    }

    pub fn with_inbound_pipeline<S>(self, inbound: S) -> Builder<S, TOutSvc, TOutReq>
    where S: Service<InboundMessage> + Clone + Send + 'static {
        Builder {
            inbound: Some(inbound),

            max_concurrent_inbound_tasks: self.max_concurrent_inbound_tasks,
            max_concurrent_outbound_tasks: self.max_concurrent_outbound_tasks,
            outbound_rx: self.outbound_rx,
            outbound_pipeline_factory: self.outbound_pipeline_factory,
        }
    }
}

impl<TInSvc, TOutSvc, TOutReq> Builder<TInSvc, TOutSvc, TOutReq>
where
    TOutSvc: Service<TOutReq> + Clone + Send + 'static,
    TInSvc: Service<InboundMessage> + Clone + Send + 'static,
{
    fn build_outbound(&mut self) -> Result<OutboundPipelineConfig<TOutReq, TOutSvc>, PipelineBuilderError> {
        let (out_sender, out_receiver) = mpsc::unbounded_channel();

        let in_receiver = self
            .outbound_rx
            .take()
            .ok_or(PipelineBuilderError::OutboundPipelineNotProvided)?;
        let factory = self
            .outbound_pipeline_factory
            .take()
            .ok_or(PipelineBuilderError::OutboundPipelineNotProvided)?;
        let sink_service = SinkService::new(out_sender);
        let pipeline = (factory)(sink_service);
        Ok(OutboundPipelineConfig {
            in_receiver,
            out_receiver: Some(out_receiver),
            pipeline,
        })
    }

    /// Try build the Pipeline
    pub fn try_finish(mut self) -> Result<Config<TInSvc, TOutSvc, TOutReq>, PipelineBuilderError> {
        let inbound = self.inbound.take().ok_or(PipelineBuilderError::InboundNotProvided)?;
        let outbound = self.build_outbound()?;

        Ok(Config {
            max_concurrent_inbound_tasks: self.max_concurrent_inbound_tasks,
            max_concurrent_outbound_tasks: self.max_concurrent_outbound_tasks,
            inbound,
            outbound,
        })
    }

    /// Builds the pipeline.
    ///
    /// ## Panics
    /// This panics if the pipeline has not been configured coorrectly.
    pub fn build(self) -> Config<TInSvc, TOutSvc, TOutReq> {
        self.try_finish().unwrap()
    }
}

/// Configuration for the outbound pipeline.
pub struct OutboundPipelineConfig<TInItem, TPipeline> {
    /// Messages read from this stream are passed to the pipeline
    pub in_receiver: mpsc::Receiver<TInItem>,
    /// Receiver of `OutboundMessage`s coming from the pipeline
    pub out_receiver: Option<mpsc::UnboundedReceiver<OutboundMessage>>,
    /// The pipeline (`tower::Service`) to run for each in_stream message
    pub pipeline: TPipeline,
}

/// Configuration for the pipeline.
pub struct Config<TInSvc, TOutSvc, TOutReq> {
    pub max_concurrent_inbound_tasks: usize,
    pub max_concurrent_outbound_tasks: Option<usize>,
    pub inbound: TInSvc,
    pub outbound: OutboundPipelineConfig<TOutReq, TOutSvc>,
}

/// Error type for the pipeline.
#[derive(Debug, Error)]
pub enum PipelineBuilderError {
    #[error("Inbound pipeline was not provided")]
    InboundNotProvided,
    #[error("Outbound pipeline was not provided")]
    OutboundPipelineNotProvided,
}

#[cfg(test)]
mod test {
    use std::convert::identity;

    use futures::future;
    use tower::service_fn;

    use super::*;

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
            .build();

        assert_eq!(config.max_concurrent_inbound_tasks, 50);
    }
}
