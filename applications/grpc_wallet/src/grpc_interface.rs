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

use crate::grpc_interface::wallet_rpc::{
    server,
    Contact as ContactRpc,
    Contacts as ContactsRpc,
    PublicKey as PublicKeyRpc,
    ReceivedTextMessage as ReceivedTextMessageRpc,
    RpcResponse,
    ScreenName as ScreenNameRpc,
    SentTextMessage as SentTextMessageRpc,
    TextMessageToSend as TextMessageToSendRpc,
    TextMessagesResponse as TextMessagesResponseRpc,
    VoidParams,
};

use futures::future;
use log::*;
use std::{convert::From, sync::Arc};
use tari_comms::{connection::NetAddress, peer_manager::Peer, types::CommsPublicKey};
use tari_crypto::tari_utilities::hex::Hex;
use tari_wallet::{
    text_message_service::{Contact, ReceivedTextMessage, SentTextMessage, UpdateContact},
    Wallet,
};
use tower_grpc::{Request, Response};

const LOG_TARGET: &str = "applications::grpc_wallet";

pub mod wallet_rpc {
    include!(concat!(env!("OUT_DIR"), "/wallet_rpc.rs"));
}

impl From<ReceivedTextMessage> for ReceivedTextMessageRpc {
    fn from(m: ReceivedTextMessage) -> Self {
        ReceivedTextMessageRpc {
            id: m.id.to_hex(),
            source_pub_key: m.source_pub_key.to_hex(),
            dest_pub_key: m.dest_pub_key.to_hex(),
            message: m.message,
            timestamp: m.timestamp.to_string(),
        }
    }
}

impl From<SentTextMessage> for SentTextMessageRpc {
    fn from(m: SentTextMessage) -> Self {
        SentTextMessageRpc {
            id: m.id.to_hex(),
            source_pub_key: m.source_pub_key.to_hex(),
            dest_pub_key: m.dest_pub_key.to_hex(),
            message: m.message,
            timestamp: m.timestamp.to_string(),
            acknowledged: m.acknowledged,
        }
    }
}

#[derive(Clone)]
pub struct WalletRPC {
    pub wallet: Arc<Wallet>,
    pub runtime: Runtime,
}

/// Implementation of the the gRPC service methods.
impl server::WalletRpc for WalletRPC {
    type AddContactFuture = future::FutureResult<Response<RpcResponse>, tower_grpc::Status>;
    type GetContactsFuture = future::FutureResult<Response<ContactsRpc>, tower_grpc::Status>;
    type GetPublicKeyFuture = future::FutureResult<Response<PublicKeyRpc>, tower_grpc::Status>;
    type GetScreenNameFuture = future::FutureResult<Response<ScreenNameRpc>, tower_grpc::Status>;
    type GetTextMessagesByContactFuture = future::FutureResult<Response<TextMessagesResponseRpc>, tower_grpc::Status>;
    type GetTextMessagesFuture = future::FutureResult<Response<TextMessagesResponseRpc>, tower_grpc::Status>;
    type RemoveContactFuture = future::FutureResult<Response<RpcResponse>, tower_grpc::Status>;
    type SendTextMessageFuture = future::FutureResult<Response<RpcResponse>, tower_grpc::Status>;
    type SetScreenNameFuture = future::FutureResult<Response<RpcResponse>, tower_grpc::Status>;
    type UpdateContactFuture = future::FutureResult<Response<RpcResponse>, tower_grpc::Status>;

    fn send_text_message(&mut self, request: Request<TextMessageToSendRpc>) -> Self::SendTextMessageFuture {
        trace!(
            target: LOG_TARGET,
            "SendTextMessage gRPC Request received: {:?}",
            request,
        );

        let msg = request.into_inner();

        let response = match CommsPublicKey::from_hex(msg.dest_pub_key.as_str()) {
            Ok(pk) => match self.wallet.text_message_service.send_text_message(pk, msg.message) {
                Ok(()) => Response::new(RpcResponse {
                    success: true,
                    message: "Text Message Sent".to_string(),
                }),
                Err(e) => Response::new(RpcResponse {
                    success: false,
                    message: format!("Error sending text message: {:?}", e).to_string(),
                }),
            },

            Err(e) => Response::new(RpcResponse {
                success: false,
                message: format!("Error sending text message: {:?}", e).to_string(),
            }),
        };

        future::ok(response)
    }

    fn get_text_messages(&mut self, request: Request<VoidParams>) -> Self::GetTextMessagesFuture {
        trace!(
            target: LOG_TARGET,
            "GetTextMessages gRPC Request received: {:?}",
            request
        );

        let response_body = match self.wallet.text_message_service.get_text_messages() {
            Ok(mut msgs) => TextMessagesResponseRpc {
                sent_messages: msgs.sent_messages.drain(..).map(|m| m.into()).collect(),
                received_messages: msgs.received_messages.drain(..).map(|m| m.into()).collect(),
            },
            _ => TextMessagesResponseRpc {
                sent_messages: Vec::new(),
                received_messages: Vec::new(),
            },
        };
        let response = Response::new(response_body);

        future::ok(response)
    }

    fn get_text_messages_by_contact(&mut self, request: Request<ContactRpc>) -> Self::GetTextMessagesFuture {
        trace!(
            target: LOG_TARGET,
            "GetTextMessages gRPC Request received: {:?}",
            request
        );

        let msg = request.into_inner();

        let pub_key = match CommsPublicKey::from_hex(msg.pub_key.as_str()) {
            Ok(pk) => pk,
            _ => {
                return future::ok(Response::new(TextMessagesResponseRpc {
                    sent_messages: Vec::new(),
                    received_messages: Vec::new(),
                }))
            },
        };

        let response_body = match self.wallet.text_message_service.get_text_messages_by_pub_key(pub_key) {
            Ok(mut msgs) => TextMessagesResponseRpc {
                sent_messages: msgs.sent_messages.drain(..).map(|m| m.into()).collect(),
                received_messages: msgs.received_messages.drain(..).map(|m| m.into()).collect(),
            },
            _ => TextMessagesResponseRpc {
                sent_messages: Vec::new(),
                received_messages: Vec::new(),
            },
        };
        let response = Response::new(response_body);

        future::ok(response)
    }

    fn set_screen_name(&mut self, request: Request<ScreenNameRpc>) -> Self::SetScreenNameFuture {
        trace!(target: LOG_TARGET, "SetScreenName gRPC Request received: {:?}", request,);

        let msg = request.into_inner();

        let response = match self.wallet.text_message_service.set_screen_name(msg.screen_name) {
            Ok(()) => Response::new(RpcResponse {
                success: true,
                message: "Screen Name Set".to_string(),
            }),
            Err(e) => Response::new(RpcResponse {
                success: false,
                message: format!("Error setting screen name: {:?}", e).to_string(),
            }),
        };

        future::ok(response)
    }

    fn get_screen_name(&mut self, request: Request<VoidParams>) -> Self::GetScreenNameFuture {
        trace!(target: LOG_TARGET, "GetScreenName gRPC Request received: {:?}", request,);

        let _msg = request.into_inner();

        let screen_name = self
            .wallet
            .text_message_service
            .get_screen_name()
            .unwrap_or_else(|_| Some("".to_string())) // Unwrap result
            .unwrap_or_else(|| "".to_string()); // Unwrap Option

        future::ok(Response::new(ScreenNameRpc { screen_name }))
    }

    fn get_public_key(&mut self, request: Request<VoidParams>) -> Self::GetPublicKeyFuture {
        trace!(target: LOG_TARGET, "GetPublicKey gRPC Request received: {:?}", request,);

        let _msg = request.into_inner();

        let public_key = self.wallet.public_key.clone().to_hex();

        future::ok(Response::new(PublicKeyRpc { pub_key: public_key }))
    }

    fn add_contact(&mut self, request: Request<ContactRpc>) -> Self::AddContactFuture {
        trace!(target: LOG_TARGET, "AddContact gRPC Request received: {:?}", request,);

        let msg = request.into_inner();

        let screen_name = msg.screen_name.clone();
        let pub_key = match CommsPublicKey::from_hex(msg.pub_key.as_str()) {
            Ok(pk) => pk,
            _ => {
                return future::ok(Response::new(RpcResponse {
                    success: false,
                    message: "Failed to add contact, cannot serialize public key".to_string(),
                }))
            },
        };

        let net_address = match msg.address.clone().parse::<NetAddress>() {
            Ok(n) => n,
            Err(e) => {
                return future::ok(Response::new(RpcResponse {
                    success: false,
                    message: format!("Failed to add contact, cannot parse net address: {:?}", e).to_string(),
                }))
            },
        };

        let peer = match Peer::from_public_key_and_address(pub_key.clone(), net_address.clone()) {
            Ok(p) => p,
            Err(e) => {
                return future::ok(Response::new(RpcResponse {
                    success: false,
                    message: format!("Failed to add contact, cannot create peer: {:?}", e).to_string(),
                }))
            },
        };

        match self.wallet.comms_services.peer_manager().add_peer(peer) {
            Err(e) => {
                return future::ok(Response::new(RpcResponse {
                    success: false,
                    message: format!("Failed to add contact, cannot add peer to Peer Manager: {:?}", e).to_string(),
                }))
            },
            _ => (),
        };

        match self.wallet.text_message_service.add_contact(Contact {
            screen_name,
            pub_key,
            address: net_address,
        }) {
            Ok(()) => (),
            Err(e) => {
                return future::ok(Response::new(RpcResponse {
                    success: false,
                    message: format!("Error adding contact: {:?}", e).to_string(),
                }))
            },
        };

        future::ok(Response::new(RpcResponse {
            success: true,
            message: "Successfully added contact".to_string(),
        }))
    }

    fn remove_contact(&mut self, request: Request<ContactRpc>) -> Self::RemoveContactFuture {
        trace!(target: LOG_TARGET, "RemoveContact gRPC Request received: {:?}", request,);

        let msg = request.into_inner();

        let net_address = match msg.address.clone().parse::<NetAddress>() {
            Ok(n) => n,
            Err(e) => {
                return future::ok(Response::new(RpcResponse {
                    success: false,
                    message: format!("Failed to remove contact, cannot parse net address: {:?}", e).to_string(),
                }))
            },
        };

        let screen_name = msg.screen_name.clone();

        if let Ok(pk) = CommsPublicKey::from_hex(msg.pub_key.as_str()) {
            let response = match self.wallet.text_message_service.remove_contact(Contact {
                screen_name,
                pub_key: pk,
                address: net_address,
            }) {
                Ok(()) => Response::new(RpcResponse {
                    success: true,
                    message: "Successfully removed contact".to_string(),
                }),
                Err(e) => Response::new(RpcResponse {
                    success: false,
                    message: format!("Error removing contact: {:?}", e).to_string(),
                }),
            };

            return future::ok(response);
        } else {
            return future::ok(Response::new(RpcResponse {
                success: false,
                message: "Failed to remove contact, cannot serialize public key".to_string(),
            }));
        }
    }

    fn update_contact(&mut self, request: Request<ContactRpc>) -> Self::RemoveContactFuture {
        trace!(target: LOG_TARGET, "UpdateContact gRPC Request received: {:?}", request,);

        let msg = request.into_inner();
        let net_address = match msg.address.clone().parse::<NetAddress>() {
            Ok(n) => n,
            Err(e) => {
                return future::ok(Response::new(RpcResponse {
                    success: false,
                    message: format!("Failed to update contact, cannot parse net address: {:?}", e).to_string(),
                }))
            },
        };
        let screen_name = msg.screen_name.clone();
        if let Ok(pk) = CommsPublicKey::from_hex(msg.pub_key.as_str()) {
            let response = match self.wallet.text_message_service.update_contact(pk, UpdateContact {
                screen_name: Some(screen_name),
                address: Some(net_address),
            }) {
                Ok(()) => Response::new(RpcResponse {
                    success: true,
                    message: "Successfully updated contact".to_string(),
                }),
                Err(e) => Response::new(RpcResponse {
                    success: false,
                    message: format!("Error updating contact: {:?}", e).to_string(),
                }),
            };

            return future::ok(response);
        } else {
            return future::ok(Response::new(RpcResponse {
                success: false,
                message: "Failed to update contact, cannot serialize public key".to_string(),
            }));
        }
    }

    fn get_contacts(&mut self, request: Request<VoidParams>) -> Self::GetContactsFuture {
        trace!(target: LOG_TARGET, "GetContacts gRPC Request received: {:?}", request,);

        let mut contacts_resp: Vec<ContactRpc> = Vec::new();

        if let Ok(contacts) = self.wallet.text_message_service.get_contacts() {
            for c in contacts.iter() {
                let sn = c.screen_name.clone();
                let address = format!("{}", c.address.clone());

                contacts_resp.push(ContactRpc {
                    screen_name: sn,
                    pub_key: c.pub_key.to_hex(),
                    address,
                });
            }
        }

        future::ok(Response::new(ContactsRpc {
            contacts: contacts_resp,
        }))
    }
}
