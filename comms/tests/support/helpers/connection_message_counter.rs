//  Copyright 2019 The Tari Project
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

use log::*;

use tari_comms::connection::{zmq::ZmqEndpoint, Connection, Context, Direction};

use std::{
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

const LOG_TARGET: &'static str = "comms::test_support::connection_message_counter";

pub struct ConnectionMessageCounter<'c> {
    counter: Arc<RwLock<u32>>,
    context: &'c Context,
}

impl<'c> ConnectionMessageCounter<'c> {
    pub fn new(context: &'c Context) -> Self {
        Self {
            counter: Arc::new(RwLock::new(0)),
            context,
        }
    }

    #[allow(dead_code)]
    pub fn reset(&self) {
        let mut counter_lock = acquire_write_lock!(self.counter);
        *counter_lock = 0;
    }

    pub fn count(&self) -> u32 {
        let counter_lock = acquire_read_lock!(self.counter);
        *counter_lock
    }

    pub fn assert_count(&self, count: u32, timeout_ms: u64) -> () {
        for _i in 0..timeout_ms {
            thread::sleep(Duration::from_millis(1));
            let curr_count = self.count();
            if curr_count == count {
                return;
            }
            if curr_count > count {
                panic!(
                    "Message count exceeded the expected count. Expected={} Actual={}",
                    count, curr_count
                );
            }
        }
        panic!(
            "Message count did not reach {} within {}ms. Count={}",
            count,
            timeout_ms,
            self.count()
        );
    }

    pub fn start<A: ZmqEndpoint + Send + Sync + Clone + 'static>(&self, address: A) -> () {
        let counter = self.counter.clone();
        let context = self.context.clone();
        let address = address.clone();
        thread::spawn(move || {
            let connection = Connection::new(&context, Direction::Inbound)
                .establish(&address)
                .unwrap();

            loop {
                match connection.receive(1000) {
                    Ok(_) => {
                        let mut counter_lock = acquire_write_lock!(counter);
                        *counter_lock += 1;
                        debug!(target: LOG_TARGET, "Added to message count (count={})", *counter_lock);
                    },
                    _ => {
                        debug!(target: LOG_TARGET, "Nothing received for 1 second...");
                    },
                }
            }
        });
    }
}
