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
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use log::debug;
use serde::Serialize;
use tokio::time::{sleep, Duration};

use super::{SyncProgress, SyncProgressInfo, SyncType, BLOCKS_SYNC_EXPECTED_TIME_SEC};
use crate::grpc::HEADERS_SYNC_EXPECTED_TIME_SEC;

fn calculate_remaining_time_in_sec(current_progress: f32, elapsed_time_in_sec: f32) -> f32 {
    elapsed_time_in_sec * (100.0 - current_progress) / current_progress
}

#[tokio::test]
async fn progress_info_test() {
    let local = 250;
    let tip = 1250;
    let sleep_sec = 5;
    let mut progress_info = SyncProgress::new(SyncType::Header, 0, 0);
    assert!(!progress_info.started);
    progress_info.start(local, tip);
    assert!(progress_info.started);
    let max_time_interval = HEADERS_SYNC_EXPECTED_TIME_SEC / 10;
    for i in 1..11 {
        let local_height = local + i * (tip - local) / 10;
        println!("iteration: {}, blocks: {}", i, local_height);
        sleep(Duration::from_secs(sleep_sec)).await;
        progress_info.sync(local_height, tip);
        let progress = SyncProgressInfo::from(progress_info.clone());
        println!("Progress: {:?}", progress);
        assert_eq!(
            HEADERS_SYNC_EXPECTED_TIME_SEC - i * max_time_interval,
            progress.max_estimated_time_sec
        );
        assert_eq!((10 - i) * sleep_sec, progress.min_estimated_time_sec);
        assert_eq!(i * sleep_sec, progress.elapsed_time_sec);
        assert_eq!(local + i * 100, progress.synced_items);
        let actual_total_items = progress.total_items - progress.starting_items_index;
        let actual_synced_items = progress.synced_items - progress.starting_items_index;
        let progress_percentage = actual_synced_items as f32 / actual_total_items as f32;
        assert_eq!(i as f32 / 10.0, progress_percentage);
    }
}

#[tokio::test]
async fn tip_height_is_changed_test() {
    let mut header_progress = SyncProgress::new(SyncType::Header, 0, 0);
    assert!(!header_progress.started);
    header_progress.start(250, 1250);
    assert!(header_progress.started);
    sleep(Duration::from_secs(5)).await;
    header_progress.sync(750, 1250);
    let progress = SyncProgressInfo::from(header_progress.clone());
    assert_eq!(5, progress.min_estimated_time_sec);
    assert_eq!(HEADERS_SYNC_EXPECTED_TIME_SEC / 2, progress.max_estimated_time_sec);
    assert_eq!(750, progress.synced_items);
    assert_eq!(5, progress.elapsed_time_sec);
    sleep(Duration::from_secs(5)).await;
    header_progress.sync(1250, 2250);
    let progress = SyncProgressInfo::from(header_progress.clone());
    println!("Progress: {:?}", progress);
    assert_eq!(10, progress.min_estimated_time_sec);
    assert_eq!(HEADERS_SYNC_EXPECTED_TIME_SEC / 2, progress.max_estimated_time_sec);
    assert_eq!(1250, progress.synced_items);
    assert_eq!(10, progress.elapsed_time_sec);
}
