#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
//! # Service framework
//!
//! This module contains the building blocks for async services.
//!
//! It consists of the following modules:
//!
//! ## `initializer`
//!
//! This module contains the [ServiceInitializer] trait. Service modules should implement this trait and pass
//! that implementation to the [StackBuilder].
//!
//! ## `stack`
//!
//! Contains the [StackBuilder] that is responsible for collecting and 'executing' the implementations of
//! [ServiceInitializer].
//!
//! ## `handles`
//!
//! A set of utilities used to collect and share handles between services. The [StackBuilder] is responsible for
//! initializing a [ServiceHandlesFuture] and making it available to [ServiceInitializer] implementations.
//!
//! Handles are simply a way to communicate with their corresponding service. Typically, a [SenderService] would
//! be used for this purpose but a handle can be implemented in any way the implementor sees fit.
//!
//! ## `reply_channel`
//!
//! This provides for query messages to be sent to services along with a "reply channel" for the service to send back
//! results. The `reply_channel::unbounded` function is used to create a sender/receiver pair. The sender
//! implements `tower_service::Service` and can be used to make requests of a applicable type. The receiver
//! implements `futures::Stream` and will provide a `RequestContext` object that contains a `oneshot` reply channel
//! that the service can use to reply back to the caller.
//!
//! ## Examples
//!
//! ### `reply_channel`
//!
//! ```edition2018
//! # use futures::executor::block_on;
//! # use futures::StreamExt;
//! # use futures::join;
//! use tari_service_framework::{reply_channel, tower::ServiceExt};
//!
//! block_on(async {
//!    let (mut sender, mut receiver) = reply_channel::unbounded();
//!
//!    let (result, _) = futures::join!(
//!         // Make the request and make progress on the resulting future
//!         sender.call_ready("upper"),
//!         // At the same time receive the request and reply
//!         async move {
//!           let req_context = receiver.next().await.unwrap();
//!           let msg = req_context.request().unwrap().clone();
//!           req_context.reply(msg.to_uppercase());
//!         }
//!     );
//!
//!    assert_eq!(result.unwrap(), "UPPER");
//! });
//! ```
//!
//! [ServiceInitializer]: ./initializer/trait.ServiceInitializer.html
//! [StackBuilder]: ./stack/struct.StackBuilder.html
//! [ServiceHandlesFuture]: ./handles/future/struct.ServiceHandlesFuture.html
//! [SenderService]: ./reply_channel/struct.SenderService.html

// Used to eliminate the need for boxing futures in many cases.
// Tracking issue: https://github.com/rust-lang/rust/issues/63063
#![feature(type_alias_impl_trait)]

mod initializer;
mod stack;

pub mod handles;
pub mod reply_channel;
pub mod tower;

pub use self::{
    initializer::{ServiceInitializationError, ServiceInitializer},
    reply_channel::RequestContext,
    stack::StackBuilder,
};
