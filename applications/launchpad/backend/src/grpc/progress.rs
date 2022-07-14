//  Copyright 2022, The Tari Project
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

use std::{
    cell::RefCell,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use log::{debug, warn};
use serde::Serialize;
use tari_app_grpc::tari_rpc::{SyncProgressResponse, SyncState};

use crate::grpc::SyncType;

pub const BLOCKS_SYNC_EXPECTED_TIME: Duration = Duration::from_secs(4 * 3600);
pub const HEADERS_SYNC_EXPECTED_TIME: Duration = Duration::from_secs(2 * 3600);

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgressInfo {
    pub sync_type: SyncType,
    pub header_progress: u64,
    pub block_progress: u64,
    pub total_blocks: u64,
    pub estimated_time_sec: u64,
    pub done: bool,
}

pub struct SyncProgress {
    sync_type: SyncType,
    header_sync: ItemCount,
    blocks_sync: ItemCount,
}

struct ItemCount {
    total_items: u64,
    start_item: u64,
    current: u64,
    started: Instant,
    initial_estimate: Duration,
    completed: Option<Duration>,
}

impl ItemCount {
    pub fn new(start_item: u64, total_items: u64, initial_estimate: Duration) -> Self {
        Self {
            total_items,
            start_item,
            current: 0,
            started: Instant::now(),
            initial_estimate,
            completed: None,
        }
    }

    pub fn update(&mut self, new_current: u64) {
        self.current = new_current;
        if self.is_done() && self.completed.is_none() {
            self.completed = Some(self.started.elapsed());
        }
    }

    pub fn is_done(&self) -> bool {
        self.current >= self.total_items
    }

    pub fn set_done(&mut self) {
        self.update(self.total_items);
    }

    pub fn estimate_remaining_time(&self) -> Duration {
        if self.is_done() {
            return Duration::from_secs(0);
        }
        let frac_complete = self.progress();
        // Use the first 1% to calibrate
        if frac_complete < 0.01 {
            self.initial_estimate
        } else {
            let elapsed = self.started.elapsed();
            Duration::from_secs_f64(elapsed.as_secs_f64() / frac_complete)
        }
    }

    pub fn progress(&self) -> f64 {
        let total_items_to_sync = self.total_items.saturating_sub(self.start_item).max(1);
        self.current.saturating_sub(self.start_item) as f64 / total_items_to_sync as f64
    }

    pub fn total_sync_time(&self) -> Option<Duration> {
        self.completed
    }
}

impl SyncProgress {
    pub fn new(starting_block: u64, total_count: u64) -> Self {
        Self {
            sync_type: SyncType::Startup,
            header_sync: ItemCount::new(starting_block, total_count, HEADERS_SYNC_EXPECTED_TIME),
            blocks_sync: ItemCount::new(starting_block, total_count, BLOCKS_SYNC_EXPECTED_TIME),
        }
    }

    fn reset(item: &mut ItemCount, current: u64, total: u64) {
        item.start_item = current;
        item.current = current;
        item.total_items = total;
        item.started = Instant::now();
    }

    pub fn update(&mut self, progress: SyncProgressResponse) {
        // Update state machine based on local sync type, reported sync type combo
        match (&self.sync_type, progress.state()) {
            (SyncType::Startup, SyncState::Header) => {
                Self::reset(&mut self.header_sync, progress.local_height, progress.tip_height);
                Self::reset(&mut self.blocks_sync, 0, progress.tip_height);
                self.sync_type = SyncType::Header;
            },
            (SyncType::Startup, SyncState::Block) => {
                Self::reset(&mut self.blocks_sync, progress.local_height, progress.tip_height);
                self.header_sync.set_done();
                self.sync_type = SyncType::Block;
            },
            (SyncType::Startup, SyncState::Done) => {
                self.header_sync.set_done();
                self.blocks_sync.set_done();
                self.sync_type = SyncType::Done;
            },
            (SyncType::Header, SyncState::Header) => {
                self.header_sync.update(progress.local_height);
            },
            (SyncType::Header, SyncState::Block) => {
                self.header_sync.set_done();
                self.sync_type = SyncType::Block;
                let last_block = self.blocks_sync.current;
                Self::reset(&mut self.blocks_sync, last_block, progress.tip_height);
                self.blocks_sync.update(progress.local_height)
            },
            (SyncType::Block, SyncState::Block) => {
                self.blocks_sync.update(progress.local_height);
            },
            // Oh no, we've gone back to header syncs
            (SyncType::Block | SyncType::Done, SyncState::Header) => {
                self.sync_type = SyncType::Header;
                self.header_sync.total_items = progress.tip_height;
                self.blocks_sync.total_items = progress.tip_height;
                self.header_sync.update(progress.local_height);
                // Leave block sync where it was
            },
            // Oh no, we've gone back to block syncs
            (SyncType::Done, SyncState::Block) => {
                self.sync_type = SyncType::Block;
                self.blocks_sync.total_items = progress.tip_height;
                self.blocks_sync.update(progress.local_height);
            },
            (_, SyncState::Done) => {
                if !self.header_sync.is_done() {
                    self.header_sync.set_done();
                }
                if !self.blocks_sync.is_done() {
                    self.blocks_sync.set_done();
                }
                self.sync_type = SyncType::Done;
            },
            _ => {
                // no-op
            },
        }
    }

    pub fn is_done(&self) -> bool {
        self.header_sync.is_done() && self.blocks_sync.is_done()
    }

    pub fn estimated_time_remaining(&self) -> Duration {
        self.blocks_sync.estimate_remaining_time() + self.header_sync.estimate_remaining_time()
    }

    pub fn progress_info(&self) -> SyncProgressInfo {
        SyncProgressInfo {
            sync_type: self.sync_type.clone(),
            header_progress: (self.header_sync.progress() * 100.0) as u64,
            block_progress: (self.blocks_sync.progress() * 100.0) as u64,
            total_blocks: self.blocks_sync.total_items,
            estimated_time_sec: self.estimated_time_remaining().as_secs(),
            done: self.is_done(),
        }
    }
}

#[cfg(test)]
mod test {
    use std::{ops::Sub, time::Duration};

    use tari_app_grpc::tari_rpc::{SyncProgressResponse, SyncState};

    use crate::grpc::{
        model::BLOCK,
        SyncProgress,
        SyncProgressInfo,
        SyncType,
        BLOCKS_SYNC_EXPECTED_TIME,
        DONE,
        HEADER,
        HEADERS_SYNC_EXPECTED_TIME,
    };

    fn almost_equal(a: Duration, b: Duration) -> bool {
        let diff = if a > b { a - b } else { b - a };
        diff.as_millis() < 10
    }

    fn confirm_progress(
        progress: SyncProgressInfo,
        sync_type: SyncType,
        header_prog: u64,
        block_prog: u64,
        total_blocks: u64,
        time: Option<u64>,
        done: bool,
    ) {
        assert_eq!(progress.sync_type, sync_type);
        assert_eq!(progress.header_progress, header_prog, "Header progress");
        assert_eq!(progress.block_progress, block_prog, "Block progress");
        assert_eq!(progress.total_blocks, total_blocks, "Total blocks");
        if let Some(t) = time {
            assert_eq!(progress.estimated_time_sec, t);
        }
        assert_eq!(progress.done, done, "Done values don't match");
    }

    #[test]
    fn initial_time_estimate() {
        let progress = SyncProgress::new(0, 50);
        assert!(matches!(progress.sync_type, SyncType::Startup));
        assert_eq!(
            progress.estimated_time_remaining(),
            BLOCKS_SYNC_EXPECTED_TIME + HEADERS_SYNC_EXPECTED_TIME
        );
    }

    #[test]
    fn instant_header_sync_time_estimate() {
        let mut progress = SyncProgress::new(0, 50);
        progress.update(SyncProgressResponse {
            state: BLOCK,
            tip_height: 50,
            local_height: 0,
        });
        assert!(matches!(progress.sync_type, SyncType::Block));
        assert!(almost_equal(
            progress.estimated_time_remaining(),
            BLOCKS_SYNC_EXPECTED_TIME
        ));
    }

    #[test]
    fn sync_flow() {
        let mut progress = SyncProgress::new(0, 50);
        progress.update(SyncProgressResponse {
            state: HEADER,
            tip_height: 20,
            local_height: 0,
        });

        progress.update(SyncProgressResponse {
            state: HEADER,
            tip_height: 20,
            local_height: 1,
        });
        confirm_progress(progress.progress_info(), SyncType::Header, 5, 0, 20, None, false);

        progress.update(SyncProgressResponse {
            state: HEADER,
            tip_height: 20,
            local_height: 20,
        });
        confirm_progress(progress.progress_info(), SyncType::Header, 100, 0, 20, None, false);

        progress.update(SyncProgressResponse {
            state: BLOCK,
            tip_height: 20,
            local_height: 7,
        });
        confirm_progress(progress.progress_info(), SyncType::Block, 100, 35, 20, None, false);

        progress.update(SyncProgressResponse {
            state: BLOCK,
            tip_height: 20,
            local_height: 20,
        });
        confirm_progress(progress.progress_info(), SyncType::Block, 100, 100, 20, None, true);

        progress.update(SyncProgressResponse {
            state: DONE,
            tip_height: 0,
            local_height: 0,
        });
        confirm_progress(progress.progress_info(), SyncType::Done, 100, 100, 20, None, true);
    }

    #[test]
    fn restart_sync() {
        // Scenario: 50 blocks
        // Sync 30 headers. Then stop the app.
        // Test that we can start up where we left off
        let mut progress = SyncProgress::new(30, 50);
        progress.update(SyncProgressResponse {
            state: HEADER,
            tip_height: 50,
            local_height: 30,
        });
        confirm_progress(progress.progress_info(), SyncType::Header, 0, 0, 50, None, false);

        progress.update(SyncProgressResponse {
            state: HEADER,
            tip_height: 50,
            local_height: 40,
        });
        confirm_progress(progress.progress_info(), SyncType::Header, 50, 0, 50, None, false);

        // If we have already started syncing blocks by next update, we correctly capture this
        progress.update(SyncProgressResponse {
            state: BLOCK,
            tip_height: 50,
            local_height: 5,
        });
        confirm_progress(progress.progress_info(), SyncType::Block, 100, 10, 50, None, false);
    }
}
