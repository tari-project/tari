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

use std::{ops::Deref, time::Duration};

use log::*;
use markup5ever::{local_name, namespace_url, ns, QualName};
use scraper::{Html, Selector};
use tokio::time::timeout;
use url::Url;

use crate::error::MmProxyError;

const LOG_TARGET: &str = "minotari_mm_proxy::monero_detect";

/// Monero public server information
#[derive(Debug, Clone, Default)]
pub struct MonerodEntry {
    /// The type of address
    pub address_type: String,
    /// The URL of the server
    pub url: String,
    /// The monero blockchain height reported by the server
    pub height: u64,
    /// Whether the server is currently up
    pub up: bool,
    /// Whether the server is web compatible
    #[allow(dead_code)]
    pub web_compatible: bool,
    /// The network the server is on (mainnet, stagenet, testnet)
    #[allow(dead_code)]
    pub network: String,
    /// Time since the server was checked
    #[allow(dead_code)]
    pub last_checked: String,
    /// The history of the server being up
    pub up_history: Vec<bool>,
    /// Response time
    pub response_time: Option<Duration>,
}

/// Get the latest monerod public nodes (by scraping the HTML frm the monero.fail website) that are
/// currently up and has a full history of being up all the time.
#[allow(clippy::too_many_lines)]
pub async fn get_monerod_info(
    number_of_entries: usize,
    connection_test_timeout: Duration,
    monero_fail_url: &str,
) -> Result<Vec<MonerodEntry>, MmProxyError> {
    let document = get_monerod_html(monero_fail_url).await?;
    let table_structure = extract_table_structure(&document);
    let expected_structure = get_expected_table_structure();
    if table_structure != expected_structure {
        return Err(MmProxyError::HtmlParseError(format!(
            "Unexpected table structure: {:?}, expected: {:?}",
            table_structure, expected_structure
        )));
    }

    // The HTML table definition and an example entry looks like this:
    //   <table class="pure-table pure-table-horizontal pure-table-striped js-sort-table">
    //       <thead>
    //           <tr>
    //               <th class="js-sort-string">Type</th>
    //               <th class="js-sort-string">URL</th>
    //               <th class="js-sort-number">Height</th>
    //               <th class="js-sort-none">Up</th>
    //               <th class="js-sort-none">Web<br/>Compatible</th>
    //               <th class="js-sort-none">Network</th>
    //               <th class="js-sort-none">Last Checked</th>
    //               <th class="js-sort-none">History</th>
    //           </tr>
    //       </thead>
    //       <tbody>
    //
    //           <tr class="js-sort-table">
    //               <td>
    //                   <img src="/static/images/tor.svg" height="20px">
    //                   <span class="hidden">tor</span>
    //               </td>
    //               <td>
    //                   <span class="nodeURL">http://node.liumin.io:18089</span>
    //               </td>
    //               <td>3119644</td>
    //               <td>
    //                   <span class="dot glowing-green"></span>
    //               </td>
    //               <td>
    //                   <img src="/static/images/error.svg" class="filter-red" width=16px>
    //               </td>
    //               <td>mainnet</td>
    //               <td>5 hours ago</td>
    //               <td>
    //                   <span class="dot glowing-green"></span>
    //                   <span class="dot glowing-green"></span>
    //                   <span class="dot glowing-green"></span>
    //                   <span class="dot glowing-green"></span>
    //                   <span class="dot glowing-green"></span>
    //                   <span class="dot glowing-green"></span>
    //               </td>
    //           </tr>

    // Define selectors for table elements
    let row_selector =
        Selector::parse("tr.js-sort-table").map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let type_selector =
        Selector::parse("td:nth-child(1)").map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let url_selector =
        Selector::parse("td:nth-child(2) .nodeURL").map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let height_selector =
        Selector::parse("td:nth-child(3)").map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let up_selector = Selector::parse("td:nth-child(4) .dot.glowing-green, td:nth-child(4) .dot.glowing-red")
        .map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let web_compatible_selector = Selector::parse("td:nth-child(5) img.filter-green, td:nth-child(5) img.filter-red")
        .map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let network_selector =
        Selector::parse("td:nth-child(6)").map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let last_checked_selector =
        Selector::parse("td:nth-child(7)").map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;
    let history_selector = Selector::parse("td:nth-child(8) .dot.glowing-green, td:nth-child(8) .dot.glowing-red")
        .map_err(|e| MmProxyError::HtmlParseError(format!("{}", e)))?;

    let mut entries = Vec::new();

    // Iterate over table rows and extract data
    for row in document.select(&row_selector) {
        let address_type = match row.select(&type_selector).next() {
            Some(val) => val.text().collect::<String>().trim().to_string(),
            None => return Err(MmProxyError::HtmlParseError("address type".to_string())),
        };

        let url = match row.select(&url_selector).next() {
            Some(val) => val.text().collect::<String>().trim().to_string(),
            None => return Err(MmProxyError::HtmlParseError("url".to_string())),
        };

        let height = match row.select(&height_selector).next() {
            Some(val) => val.text().collect::<String>().trim().parse::<u64>().unwrap_or_default(),
            None => return Err(MmProxyError::HtmlParseError("height".to_string())),
        };

        let mut up = false;
        let iter = row.select(&up_selector);
        for item in iter {
            let class = item.value().attr("class").unwrap_or("");
            if class.contains("dot glowing-green") {
                up = true;
                break;
            }
        }

        let mut web_compatible = false;
        let iter = row.select(&web_compatible_selector);
        for item in iter {
            let class = item.value().attr("class").unwrap_or("");
            if class.contains("filter-green") {
                web_compatible = true;
                break;
            }
        }

        let network = match row.select(&network_selector).next() {
            Some(val) => val.text().collect::<String>().trim().to_string(),
            None => return Err(MmProxyError::HtmlParseError("network".to_string())),
        };

        let last_checked = match row.select(&last_checked_selector).next() {
            Some(val) => val.text().collect::<String>().trim().to_string(),
            None => return Err(MmProxyError::HtmlParseError("last checked".to_string())),
        };

        let mut up_history = Vec::new();
        let iter = row.select(&history_selector);
        for item in iter {
            let class = item.value().attr("class").unwrap_or("");
            up_history.push(class.contains("dot glowing-green"));
        }

        let entry = MonerodEntry {
            address_type: address_type.to_lowercase(),
            url,
            height,
            up,
            web_compatible,
            network: network.to_lowercase(),
            last_checked,
            up_history,
            response_time: None,
        };
        entries.push(entry);
    }

    // Only retain nodes that are currently up and has a full history of being up all the time
    let max_history_length = entries.iter().map(|entry| entry.up_history.len()).max().unwrap_or(0);
    entries.retain(|entry| {
        entry.up && entry.up_history.iter().filter(|&&v| v).collect::<Vec<_>>().len() == max_history_length
    });
    // Only retain non-tor and non-i2p nodes
    entries.retain(|entry| entry.address_type != *"tor" && entry.address_type != *"i2p");
    // Give preference to nodes with the best height
    entries.sort_by(|a, b| b.height.cmp(&a.height));
    // Determine connection times - use slightly more nodes than requested
    entries.truncate(number_of_entries + 10);

    let entries = order_and_select_monerod_info(number_of_entries, connection_test_timeout, &entries).await?;

    if entries.is_empty() {
        return Err(MmProxyError::HtmlParseError(
            "No public monero servers available".to_string(),
        ));
    }
    Ok(entries)
}

pub async fn order_and_select_monerod_info(
    number_of_entries: usize,
    connection_test_timeout: Duration,
    entries: &[MonerodEntry],
) -> Result<Vec<MonerodEntry>, MmProxyError> {
    let mut entries = entries.to_vec();
    for entry in &mut entries {
        if let Ok(url) = format!("{}/getheight", entry.url).parse::<Url>() {
            let start = std::time::Instant::now();
            if (timeout(connection_test_timeout, reqwest::get(url.clone())).await).is_ok() {
                entry.response_time = Some(start.elapsed());
                debug!(
                    target: LOG_TARGET, "Response time '{:.2?}' for Monerod server at: {}",
                    entry.response_time, url.as_str()
                );
            } else {
                debug!(
                    target: LOG_TARGET, "Response time 'n/a' for Monerod server at: {}, timed out",
                    url.as_str()
                );
            }
        }
    }
    // Remove entries with no response time
    entries.retain(|entry| entry.response_time.is_some());
    // Sort by response time
    entries.sort_by(|a, b| {
        a.response_time
            .unwrap_or_else(|| Duration::from_secs(100))
            .cmp(&b.response_time.unwrap_or_else(|| Duration::from_secs(100)))
    });
    // Truncate to the requested number of entries
    entries.truncate(number_of_entries);
    Ok(entries)
}

async fn get_monerod_html(url: &str) -> Result<Html, MmProxyError> {
    let body = match reqwest::get(url).await {
        Ok(resp) => match resp.text().await {
            Ok(html) => html,
            Err(e) => {
                error!("Failed to fetch monerod info: {}", e);
                return Err(MmProxyError::MonerodRequestFailed(e));
            },
        },
        Err(e) => {
            error!("Failed to fetch monerod info: {}", e);
            return Err(MmProxyError::MonerodRequestFailed(e));
        },
    };

    Ok(Html::parse_document(&body))
}

// Function to extract table structure from the document
fn extract_table_structure(html_document: &Html) -> Vec<&str> {
    let mut table_structure = Vec::new();
    if let Some(table) = html_document.tree.root().descendants().find(|n| {
        n.value().is_element() &&
            n.value().as_element().unwrap().name == QualName::new(None, ns!(html), local_name!("table"))
    }) {
        if let Some(thead) = table.descendants().find(|n| {
            n.value().is_element() &&
                n.value().as_element().unwrap().name == QualName::new(None, ns!(html), local_name!("thead"))
        }) {
            if let Some(tr) = thead.descendants().find(|n| {
                n.value().is_element() &&
                    n.value().as_element().unwrap().name == QualName::new(None, ns!(html), local_name!("tr"))
            }) {
                for th in tr.descendants().filter(|n| {
                    n.value().is_element() &&
                        n.value().as_element().unwrap().name == QualName::new(None, ns!(html), local_name!("th"))
                }) {
                    for child in th.children() {
                        if let Some(text) = child.value().as_text() {
                            table_structure.push(text.deref().trim());
                        }
                    }
                }
            }
        }
    }
    table_structure
}

fn get_expected_table_structure<'a>() -> Vec<&'a str> {
    vec![
        "Type",
        "URL",
        "Height",
        "Up",
        "Web",
        "Compatible",
        "Network",
        "Last Checked",
        "History",
    ]
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::{
        config::MergeMiningProxyConfig,
        error::MmProxyError::HtmlParseError,
        monero_fail::{
            extract_table_structure,
            get_expected_table_structure,
            get_monerod_html,
            get_monerod_info,
            order_and_select_monerod_info,
            MonerodEntry,
        },
    };

    async fn get_monerod_info_if_online(
        number_of_entries: usize,
        connection_test_timeout: Duration,
        monero_fail_url: &str,
    ) -> Vec<MonerodEntry> {
        match get_monerod_info(number_of_entries, connection_test_timeout, monero_fail_url).await {
            Ok(val) => val,
            Err(HtmlParseError(val)) => {
                if val.contains("No public monero servers available") {
                    println!("Cannot complete test: {}", val);
                    vec![]
                } else {
                    panic!("Unexpected error: {}", val);
                }
            },
            Err(err) => {
                if err.to_string().contains("Failed to send request to monerod") {
                    println!("Cannot complete test: {}", err);
                    vec![]
                } else {
                    panic!("Unexpected error: {}", err);
                }
            },
        }
    }

    #[tokio::test]
    async fn test_get_monerod_info() {
        // Monero mainnet
        let config = MergeMiningProxyConfig::default();
        let entries = get_monerod_info_if_online(5, Duration::from_secs(2), &config.monero_fail_url).await;
        for (i, entry) in entries.iter().enumerate() {
            assert!(entry.up && entry.up_history.iter().all(|&v| v));
            if i > 0 {
                assert!(
                    entry.response_time.unwrap_or_else(|| Duration::from_secs(100)) >=
                        entries[i - 1].response_time.unwrap_or_else(|| Duration::from_secs(100))
                );
            }
            assert_eq!(entry.network, "mainnet");
            println!("{}: {:?}", i, entry);
        }

        // Monero stagenet
        const MONERO_FAIL_STAGNET_URL: &str = "https://monero.fail/?chain=monero&network=stagenet&all=true";
        let entries = get_monerod_info_if_online(5, Duration::from_secs(2), MONERO_FAIL_STAGNET_URL).await;
        for (i, entry) in entries.iter().enumerate() {
            assert!(entry.up && entry.up_history.iter().all(|&v| v));
            if i > 0 {
                assert!(
                    entry.response_time.unwrap_or_else(|| Duration::from_secs(100)) >=
                        entries[i - 1].response_time.unwrap_or_else(|| Duration::from_secs(100))
                );
            }
            println!("{}: {:?}", i, entry);
        }

        // Monero testnet
        const MONERO_FAIL_TESTNET_URL: &str = "https://monero.fail/?chain=monero&network=testnet&all=true";
        let entries = get_monerod_info_if_online(5, Duration::from_secs(2), MONERO_FAIL_TESTNET_URL).await;
        for (i, entry) in entries.iter().enumerate() {
            assert!(entry.up && entry.up_history.iter().all(|&v| v));
            if i > 0 {
                assert!(
                    entry.response_time.unwrap_or_else(|| Duration::from_secs(100)) >=
                        entries[i - 1].response_time.unwrap_or_else(|| Duration::from_secs(100))
                );
            }
            println!("{}: {:?}", i, entry);
        }
    }

    #[tokio::test]
    async fn test_default_monerod_list() {
        let config = MergeMiningProxyConfig::default();

        // Use the default monerod list
        let mut entries = Vec::new();
        for url in config.monerod_url.clone().into_vec() {
            entries.push(MonerodEntry {
                url,
                ..Default::default()
            });
        }
        let ordered_entries = order_and_select_monerod_info(10, Duration::from_secs(5), &entries)
            .await
            .unwrap();
        for (i, entry) in ordered_entries.iter().enumerate() {
            if i > 0 {
                assert!(
                    entry.response_time.unwrap_or_else(|| Duration::from_secs(100)) >=
                        ordered_entries[i - 1]
                            .response_time
                            .unwrap_or_else(|| Duration::from_secs(100))
                );
            }
            println!("{}: {:?}", i, entry);
        }

        // Use the previously qualified monerod list, but invalidate some URLs
        let ordered_entries = ordered_entries
            .iter()
            .filter(|entry| entry.url.contains(":18"))
            .cloned()
            .collect::<Vec<_>>();
        let mut entries = Vec::new();
        for (i, entry) in ordered_entries.iter().take(5).enumerate() {
            entries.push(MonerodEntry {
                url: if i >= 2 {
                    entry.url.replace(":18", ":123")
                } else {
                    entry.url.clone()
                },
                ..Default::default()
            });
        }
        let ordered_entries = order_and_select_monerod_info(15, Duration::from_secs(5), &entries)
            .await
            .unwrap();
        for (i, entry) in ordered_entries.iter().enumerate() {
            if i > 0 {
                assert!(
                    entry.response_time.unwrap_or_else(|| Duration::from_secs(100)) >=
                        ordered_entries[i - 1]
                            .response_time
                            .unwrap_or_else(|| Duration::from_secs(100))
                );
            }
            println!("{}: {:?}", i, entry);
        }
        assert_eq!(ordered_entries.len(), 2);
    }

    #[tokio::test]
    async fn test_table_structure() {
        let config = MergeMiningProxyConfig::default();
        let html_content = match get_monerod_html(&config.monero_fail_url).await {
            Ok(val) => val,
            Err(err) => {
                if err.to_string().contains("Failed to send request to monerod") {
                    println!("Cannot complete test: {}", err);
                    return;
                }
                panic!("Unexpected error: {}", err);
            },
        };

        let table_structure = extract_table_structure(&html_content);
        let expected_structure = get_expected_table_structure();

        // Compare the actual and expected table structures
        if table_structure.is_empty() {
            println!(
                "Cannot complete test, 'https://monero.fail' seems to be down ({:?})",
                html_content.errors
            );
        } else {
            assert_eq!(table_structure, expected_structure);
        }
    }
}
