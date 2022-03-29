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

#![allow(clippy::type_complexity)]

use std::sync::Arc;

use crate::{
    base_node::sync::{horizon_state_sync::HorizonSyncInfo, SyncPeer},
    blocks::ChainBlock,
};

#[derive(Default)]
pub(super) struct Hooks {
    on_starting: Vec<Box<dyn FnOnce() + Send + Sync>>,
    on_progress_header: Vec<Box<dyn Fn(u64, u64, &SyncPeer) + Send + Sync>>,
    on_progress_block: Vec<Box<dyn Fn(Arc<ChainBlock>, u64, &SyncPeer) + Send + Sync>>,
    on_progress_horizon_sync: Vec<Box<dyn Fn(HorizonSyncInfo) + Send + Sync>>,
    on_complete: Vec<Box<dyn Fn(Arc<ChainBlock>) + Send + Sync>>,
    on_rewind: Vec<Box<dyn Fn(Vec<Arc<ChainBlock>>) + Send + Sync>>,
}

impl Hooks {
    pub fn add_on_starting_hook<H>(&mut self, hook: H)
    where H: FnOnce() + Send + Sync + 'static {
        self.on_starting.push(Box::new(hook));
    }

    pub fn call_on_starting_hook(&mut self) {
        self.on_starting.drain(..).for_each(|f| (f)());
    }

    pub fn add_on_progress_header_hook<H>(&mut self, hook: H)
    where H: Fn(u64, u64, &SyncPeer) + Send + Sync + 'static {
        self.on_progress_header.push(Box::new(hook));
    }

    pub fn call_on_progress_header_hooks(&self, local_height: u64, remote_height: u64, sync_peer: &SyncPeer) {
        self.on_progress_header
            .iter()
            .for_each(|f| (*f)(local_height, remote_height, sync_peer));
    }

    pub fn add_on_progress_block_hook<H>(&mut self, hook: H)
    where H: Fn(Arc<ChainBlock>, u64, &SyncPeer) + Send + Sync + 'static {
        self.on_progress_block.push(Box::new(hook));
    }

    pub fn call_on_progress_block_hooks(&self, block: Arc<ChainBlock>, remote_tip_height: u64, sync_peer: &SyncPeer) {
        self.on_progress_block
            .iter()
            .for_each(|f| (*f)(block.clone(), remote_tip_height, sync_peer));
    }

    pub fn add_on_progress_horizon_hook<H>(&mut self, hook: H)
    where H: Fn(HorizonSyncInfo) + Send + Sync + 'static {
        self.on_progress_horizon_sync.push(Box::new(hook));
    }

    pub fn call_on_progress_horizon_hooks(&self, info: HorizonSyncInfo) {
        self.on_progress_horizon_sync.iter().for_each(|f| (*f)(info.clone()));
    }

    pub fn add_on_complete_hook<H>(&mut self, hook: H)
    where H: Fn(Arc<ChainBlock>) + Send + Sync + 'static {
        self.on_complete.push(Box::new(hook));
    }

    pub fn call_on_complete_hooks(&self, final_block: Arc<ChainBlock>) {
        self.on_complete.iter().for_each(|f| (*f)(final_block.clone()));
    }

    pub fn add_on_rewind_hook<H>(&mut self, hook: H)
    where H: Fn(Vec<Arc<ChainBlock>>) + Send + Sync + 'static {
        self.on_rewind.push(Box::new(hook));
    }

    pub fn call_on_rewind_hooks(&mut self, blocks: Vec<Arc<ChainBlock>>) {
        self.on_rewind.iter().for_each(|f| (*f)(blocks.clone()));
    }
}
