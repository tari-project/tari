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

use derive_error::Error;
use std::{collections::HashMap, error::Error, hash::Hash};

#[derive(Debug, Error, Clone)]
pub enum DispatchError {
    /// A dispatch route was not defined for the specific message type
    MessageHandlerNotDefined,
    #[error(msg_embedded, non_std, no_from)]
    ResolveFailed(String),
    #[error(msg_embedded, non_std, no_from)]
    HandlerError(String),
}

impl DispatchError {
    pub fn resolve_failed<E>() -> impl Fn(E) -> Self
    where E: Error {
        |err| DispatchError::ResolveFailed(format!("Dispatch resolve failed: {}", err))
    }

    pub fn handler_error<E>() -> impl Fn(E) -> Self
    where E: Error {
        |err| DispatchError::HandlerError(format!("Handler error: {}", err))
    }
}

#[derive(Debug, Error, Clone)]
pub enum HandlerError {
    #[error(msg_embedded, non_std, no_from)]
    Failed(String),
}

impl HandlerError {
    pub fn failed<E>() -> impl Fn(E) -> Self
    where E: Error {
        |err| HandlerError::Failed(format!("Handler failed with error '{}'", err))
    }
}

/// The signature of a handler function
type HandlerFunc<M, E = HandlerError> = fn(msg: M) -> Result<(), E>;

/// The trait bound for type parameter K on the dispatcher i.e the route key.
/// K must be Eq + Hash + {able to be sent across threads} +
/// 'static (all references must live as long as the program BUT there are no references
/// so therefore this is simply to satisfy the closure on `thread::spawn`)
/// This saves us from having to duplicate these trait bounds whenever we
/// want specify the type parameter K (like a type alias).
pub trait DispatchableKey: Eq + Hash + Send + Sync + 'static {}

/// Implement this trait for all types which satisfy it's trait bounds.
impl<T> DispatchableKey for T where T: Eq + Hash + Send + Sync + 'static {}

/// A message type resolver. The resolver is called with the dispatched message.
/// The resolver should then decide which dispatch key should be used.
pub trait DispatchResolver<K, M> //: Send + 'static
// where K: DispatchableKey
{
    fn resolve(&self, msg: &M) -> Result<K, DispatchError>;
}

/// Dispatcher pattern. Links handler function to "keys" which are resolved
/// be a given type implementing [DispatchResolver].
///
/// ## Type Parameters
/// `K` - The route key
/// `M` - The type which is passed into the handler
/// `R` - The resolver type
/// `E` - The type of error returned from the handler
pub struct Dispatcher<K, M, R, E = HandlerError>
where R: DispatchResolver<K, M>
{
    handlers: HashMap<K, HandlerFunc<M, E>>,
    catch_all: Option<HandlerFunc<M, E>>,
    resolver: R,
}

impl<K, M, R, E> Dispatcher<K, M, R, E>
where
    K: DispatchableKey,
    R: DispatchResolver<K, M>,
    E: Error,
{
    /// Construct a new MessageDispatcher with no defined dispatch routes
    pub fn new(resolver: R) -> Dispatcher<K, M, R, E> {
        Dispatcher {
            handlers: HashMap::new(),
            resolver,
            catch_all: None,
        }
    }

    /// This function allows a new dispatch route to be specified and added to the handlers, all received messaged that
    /// are of the dispatch type will be routed to the specified handler_function
    pub fn route(mut self, path_key: K, handler: HandlerFunc<M, E>) -> Self {
        self.handlers.insert(path_key, handler);
        self
    }

    /// Set the handler to use if no other handlers match
    pub fn catch_all(mut self, handler: HandlerFunc<M, E>) -> Self {
        self.catch_all = Some(handler);
        self
    }

    /// This function can be used to forward a message to the correct function handler
    pub fn dispatch(&self, msg: M) -> Result<(), DispatchError> {
        let route_type = self.resolver.resolve(&msg)?;
        self.handlers
            .get(&route_type)
            .or_else(|| self.catch_all.as_ref())
            .ok_or(DispatchError::MessageHandlerNotDefined)
            .and_then(|handler| {
                handler(msg).map_err(|err| DispatchError::HandlerError(format!("Handler error: {:?}", err)))
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_route_and_dispatch() {
        #[derive(Debug, Hash, Eq, PartialEq)]
        pub enum DispatchType {
            Unknown,
            Type1,
            Type2,
            Type3,
        }

        pub struct Message {
            pub data: String,
        }

        pub struct TestResolver;

        impl DispatchResolver<DispatchType, Message> for TestResolver {
            fn resolve(&self, msg: &Message) -> Result<DispatchType, DispatchError> {
                // Here you would usually look at the header for a message type
                Ok(match msg.data.as_ref() {
                    "Type1" => DispatchType::Type1,
                    "Type2" => DispatchType::Type2,
                    _ => DispatchType::Type3,
                })
            }
        }
        // Create a common variable to determine which handler function was called by the dispatcher
        static mut CALLED_FN_TYPE: DispatchType = DispatchType::Unknown;

        fn test_fn1(_msg_data: Message) -> Result<(), DispatchError> {
            unsafe {
                CALLED_FN_TYPE = DispatchType::Type1;
            }
            Ok(())
        }

        fn test_fn2(_msg_data: Message) -> Result<(), DispatchError> {
            unsafe {
                CALLED_FN_TYPE = DispatchType::Type2;
            }
            Ok(())
        }

        fn test_fn3(_msg_data: Message) -> Result<(), DispatchError> {
            unsafe {
                CALLED_FN_TYPE = DispatchType::Type3;
            }
            Ok(())
        }

        let resolver = TestResolver {};

        let message_dispatcher = Dispatcher::new(resolver)
            .route(DispatchType::Type1, test_fn1)
            .route(DispatchType::Type2, test_fn2)
            .route(DispatchType::Type3, test_fn3);
        // Test dispatch to default route
        let msg_data = Message { data: "".to_string() };
        assert!(message_dispatcher.dispatch(msg_data).is_ok());
        unsafe {
            assert_eq!(CALLED_FN_TYPE, DispatchType::Type3);
        }
        // Test dispatch to specified type route
        let msg_data = Message {
            data: "Type2".to_string(),
        };
        assert!(message_dispatcher.dispatch(msg_data).is_ok());
        unsafe {
            assert_eq!(CALLED_FN_TYPE, DispatchType::Type2);
        }
    }
}
