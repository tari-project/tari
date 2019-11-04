// Copyright 2019. The Tari Project
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
use crate::miner_code::Miner;
use futures::{future::FutureExt, pin_mut, select};
use log::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tari_core::blocks::{Block, BlockBuilder, BlockHeader};
use tari_transactions::consensus::ConsensusRules;
use tokio_executor::threadpool::ThreadPool;

const LOG_TARGET: &str = "base_node::miner";

// Removing GRPC for now as the basenode coms are not going to be through GRPC for this miner,
// wallet might still be so leaving the code as an example
// pub mod testnet_miner {
//     tonic::include_proto!("testnet_miner_rpc");
// }
// use testnet_miner::{client::TestNetMinerClient, BlockHeaderMessage, BlockHeight, BlockMessage, VoidParams};
// let mut base_node = TestNetMinerClient::connect(settings.base_node_address.unwrap())?;
//     let request = tonic::Request::new(VoidParams {});

//     let response = base_node.get_block(request).await?;

/// This is a blocking thread, this needs to run on its own thread.
async fn run(stop_flag: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    info!(target: LOG_TARGET, "Requesting new block");

    let mut miner = Miner::new(ConsensusRules::current());
    let mut block = get_block();
    let mining_flag = miner.get_mine_flag();

    let mut pool = ThreadPool::new();
    loop {
        let height: u64 = if block.header.height <= 2016 {
            1
        } else {
            block.header.height - 2016
        };

        miner.add_block(block);
        let header = get_blockheader();

        // create threads

        let t_miner = mine(&mut miner, header, &mut pool).fuse();
        let t_cli = stop_mining(stop_flag.clone()).fuse();
        let t_tip = check_tip(0).fuse();
        pin_mut!(t_miner);
        pin_mut!(t_cli);
        pin_mut!(t_tip);

        select! {
        () = t_miner => {info!(target: LOG_TARGET, "Mined a block");
                        send_block();
                        },
        () = t_cli => {info!(target: LOG_TARGET, "Canceled mining");
                        mining_flag.store(true, Ordering::Relaxed);
                        },
        () = t_tip => info!(target: LOG_TARGET, "Canceled mining on current block, tip changed"),}
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        block = get_block();
    }
    pool.shutdown().await;
    Ok(())
}

async fn mine(miner: &mut Miner, header: BlockHeader, pool: &mut ThreadPool) {
    miner.mine(header, pool).await;
}

async fn stop_mining(stop_flag: Arc<AtomicBool>) {
    loop {
        tokio::timer::delay(Instant::now() + Duration::from_millis(1000)).await;
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
    }
}

// This checks for a tip increase meaning we need to change to a new block for mining as we are not mining on the
// largest chain anymore. Todo this should check diff and not height.
async fn check_tip(current_tip: u64) {
    loop {
        tokio::timer::delay(Instant::now() + Duration::from_millis(1000)).await;
        if current_tip < get_chain_tip_height() {
            break;
        }
    }
}

// todo get block here
fn get_block() -> Block {
    BlockBuilder::new()
        .with_header(BlockHeader::new(ConsensusRules::current().blockchain_version()))
        .build()
}

// todo get blockheader here
fn get_blockheader() -> BlockHeader {
    BlockHeader::new(ConsensusRules::current().blockchain_version())
}

// todo get tip height here
fn get_chain_tip_height() -> u64 {
    0
}

// todo propagate block
fn send_block() {
    println!("sending mined block out");
}
