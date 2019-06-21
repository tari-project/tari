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
use tari_comms::builder::CommsRoutes;

/// This is a container for services. Services can be registered here
/// and given to the [ServiceExecutor] for execution. This also
/// builds [CommsRoutes] for the given services.
pub struct ServiceRegistry {
    pub(super) services: Vec<Box<Service>>,
}

impl ServiceRegistry {
    /// Create a new [ServiceRegistry].
    pub fn new() -> Self {
        Self { services: Vec::new() }
    }

    /// Register a [Service]
    pub fn register<S>(mut self, service: S) -> Self
    where S: Service + 'static {
        self.services.push(Box::new(service));
        self
    }

    /// Retrieves the number of services registered.
    pub fn num_services(&self) -> usize {
        self.services.len()
    }

    /// Builds the [CommsRoutes] for the services contained in this registry.
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        services::{ServiceContext, ServiceError},
        tari_message::NetMessage,
    };

    struct DummyService;

    impl Service for DummyService {
        fn get_name(&self) -> String {
            "dummy".to_string()
        }

        fn get_message_types(&self) -> Vec<TariMessageType> {
            vec![NetMessage::PingPong.into()]
        }

        fn execute(&mut self, _context: ServiceContext) -> Result<(), ServiceError> {
            // do nothing
            Ok(())
        }
    }

    #[test]
    fn num_services() {
        let mut registry = ServiceRegistry::new();
        assert_eq!(registry.num_services(), 0);
        let service = DummyService {};
        registry = registry.register(service);
        assert_eq!(registry.num_services(), 1);
    }

    #[test]
    fn build_comms_routes() {
        let service = DummyService {};
        let registry = ServiceRegistry::new().register(service);

        let routes = registry.build_comms_routes();
        routes.get_address(&NetMessage::PingPong.into()).unwrap();
    }
}
