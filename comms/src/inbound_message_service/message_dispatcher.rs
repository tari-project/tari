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
use std::collections::HashMap;

#[derive(Debug, Error)]
pub enum DispatchError {
    /// A dispatch route was not defined for the specific message type
    MessageHandlerUndefined,
}

/// Each message struct that will be dispatched should implement Dispatchable. The dispatch_type function is used by the
/// MessageDispatcher to be able to distinguish between different message types and select which handler function should
/// process the message.
pub trait Dispatchable {
    fn dispatch_type(&self) -> u32;
}

/// Format required of handler functions specified by dispatch routes
type HandlerFunctionFormat<DispMsg> = fn(msg_data: DispMsg) -> Result<(), DispatchError>;

#[derive(Clone, Debug)]
pub struct MessageDispatcher<DispMsg> {
    handlers: HashMap<u32, HandlerFunctionFormat<DispMsg>>,
}

impl<DispMsg> MessageDispatcher<DispMsg>
where DispMsg: Dispatchable
{
    /// Construct a new MessageDispatcher with no defined dispatch routes
    pub fn new() -> MessageDispatcher<DispMsg> {
        MessageDispatcher {
            handlers: HashMap::new(),
        }
    }

    /// This function allows a new dispatch route to be specified and added to the handlers, all received messaged that
    /// are of the dispatch type will be routed to the specified handler_function
    pub fn route(mut self, dispatch_type: u32, handler_function: HandlerFunctionFormat<DispMsg>) -> Self {
        self.handlers.insert(dispatch_type, handler_function);
        self
    }

    /// This function can be used to forward a message to the correct function handler
    pub fn dispatch(&self, msg_data: DispMsg) -> Result<(), DispatchError> {
        match self.handlers.get(&msg_data.dispatch_type()) {
            Some(dispatch_function) => {
                dispatch_function(msg_data)?;
                Ok(())
            },
            None => Err(DispatchError::MessageHandlerUndefined),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_route_and_dispatch() {
        #[derive(PartialEq, Debug)]
        pub enum DispatchType {
            Unused,
            Type1,
            Type2,
            Type3,
        }

        pub struct Message {
            pub data: String,
        }

        impl Dispatchable for Message {
            fn dispatch_type(&self) -> u32 {
                match self.data.as_ref() {
                    "Type1" => DispatchType::Type1 as u32,
                    "Type2" => DispatchType::Type2 as u32,
                    _ => DispatchType::Type3 as u32,
                }
            }
        }
        // Create a common variable to determine which handler function was called by the dispatcher
        static mut CALLED_FN_TYPE: DispatchType = DispatchType::Unused;

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

        let message_dispatcher = MessageDispatcher::<Message>::new()
            .route(DispatchType::Type1 as u32, test_fn1)
            .route(DispatchType::Type2 as u32, test_fn2)
            .route(DispatchType::Type3 as u32, test_fn3);
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
