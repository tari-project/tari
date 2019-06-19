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

use super::service::Service;
use crate::tari_message::TariMessageType;
use std::collections::HashMap;
use tari_comms::{builder::CommsRoutes, connection::InprocAddress};

pub struct ServiceRegistry {
    pub(super) services: Vec<Box<Service>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self { services: Vec::new() }
    }

    pub fn register<S>(mut self, service: S) -> Self
    where S: Service + 'static {
        self.services.push(Box::new(service));
        self
    }

    pub fn num_services(&self) -> usize {
        self.services.len()
    }

    pub fn build_comms_routes(&self) -> CommsRoutes<TariMessageType> {
        let mut routes = CommsRoutes::new();
        for service in &self.services {
            routes = service
                .get_message_types()
                .iter()
                .fold(routes, |routes, message_type| routes.register(message_type.clone()));
        }

        routes
    }
}
