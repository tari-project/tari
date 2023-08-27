//  Copyright 2022, The Taiji Project
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

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};
use crate::table::Table;

/// Displays network stats
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.get_network_stats()
    }
}

impl CommandContext {
    #[cfg(not(feature = "metrics"))]
    pub fn get_network_stats(&self) -> Result<(), Error> {
        println!(
            "Metrics are not enabled in this binary. Recompile Minotaiji base node with `--features metrics` to enable \
             them."
        );
        Ok(())
    }

    #[cfg(feature = "metrics")]
    pub fn get_network_stats(&self) -> Result<(), Error> {
        use taiji_metrics::proto::MetricType;
        let metric_families = taiji_metrics::get_default_registry().gather();
        let metric_family_iter = metric_families
            .into_iter()
            .filter(|family| family.get_name().starts_with("taiji_comms"));

        let mut table = Table::new();
        table.set_titles(vec!["name", "type", "value"]);
        for family in metric_family_iter {
            let field_type = family.get_field_type();
            let name = family.get_name();
            for metric in family.get_metric() {
                let value = match field_type {
                    MetricType::COUNTER => metric.get_counter().get_value(),
                    MetricType::GAUGE => metric.get_gauge().get_value(),
                    MetricType::SUMMARY => {
                        let summary = metric.get_summary();
                        summary.get_sample_sum() / summary.get_sample_count() as f64
                    },
                    MetricType::UNTYPED => metric.get_untyped().get_value(),
                    MetricType::HISTOGRAM => {
                        let histogram = metric.get_histogram();
                        histogram.get_sample_sum() / histogram.get_sample_count() as f64
                    },
                };

                let field_type = match field_type {
                    MetricType::COUNTER => "COUNTER",
                    MetricType::GAUGE => "GAUGE",
                    MetricType::SUMMARY => "SUMMARY",
                    MetricType::UNTYPED => "UNTYPED",
                    MetricType::HISTOGRAM => "HISTOGRAM",
                };

                table.add_row(row![name, field_type, value]);
            }
        }
        table.print_stdout();
        Ok(())
    }
}
