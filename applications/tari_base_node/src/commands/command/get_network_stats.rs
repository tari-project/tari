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
            "Metrics are not enabled in this binary. Recompile Tari base node with `--features metrics` to enable \
             them."
        );
        Ok(())
    }

    #[cfg(feature = "metrics")]
    pub fn get_network_stats(&self) -> Result<(), Error> {
        use tari_metrics::proto::MetricType;
        let metric_families = tari_metrics::get_default_registry().gather();
        let metric_family_iter = metric_families
            .into_iter()
            .filter(|family| family.get_name().starts_with("tari_comms"));

        // TODO: Make this useful
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
