//  Copyright 2021, The Tari Project
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

use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
pub struct Watch<T>(Arc<watch::Sender<T>>, watch::Receiver<T>);

impl<T> Watch<T> {
    pub fn new(initial: T) -> Self {
        let (tx, rx) = watch::channel(initial);
        Self(Arc::new(tx), rx)
    }

    pub fn borrow(&self) -> watch::Ref<'_, T> {
        self.receiver().borrow()
    }

    pub async fn changed(&mut self) {
        if self.1.changed().await.is_err() {
            // Result::expect requires E: fmt::Debug and `watch::SendError<T>` is not, this is equivalent
            panic!("watch internal receiver is dropped");
        }
    }

    pub fn send(&self, item: T) {
        // PANIC: broadcast becomes infallible because the receiver is owned in Watch and so the failure case is
        // unreachable
        if self.sender().send(item).is_err() {
            // Result::expect requires E: fmt::Debug and `watch::SendError<T>` is not, this is equivalent
            panic!("watch internal receiver is dropped");
        }
    }

    fn sender(&self) -> &watch::Sender<T> {
        &self.0
    }

    pub fn receiver(&self) -> &watch::Receiver<T> {
        &self.1
    }

    pub fn get_receiver(&self) -> watch::Receiver<T> {
        self.receiver().clone()
    }
}
