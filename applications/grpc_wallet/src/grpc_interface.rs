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

use crate::{
    grpc_interface::wallet_rpc::{
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
    },
    wallet_server::WalletServerError,
};

use futures::future;
use log::*;
use std::{convert::TryFrom, sync::Arc};
use tari_comms::{connection::NetAddress, peer_manager::Peer, types::CommsPublicKey};
use tari_utilities::message_format::MessageFormat;
use tari_wallet::{
    text_message_service::{Contact, TextMessage, TextMessages, UpdateContact},
    Wallet,
};
use tower_grpc::{Request, Response};

const LOG_TARGET: &'static str = "applications::grpc_wallet";

pub mod wallet_rpc {
    include!(concat!(env!("OUT_DIR"), "/wallet_rpc.rs"));
}

impl TryFrom<TextMessage> for ReceivedTextMessageRpc {
    type Error = WalletServerError;

    fn try_from(m: TextMessage) -> Result<Self, Self::Error> {
        Ok(ReceivedTextMessageRpc {
            id: m.id.to_base64().unwrap(),
            source_pub_key: m.source_pub_key.to_base64()?,
            dest_pub_key: m.dest_pub_key.to_base64()?,
            message: m.message,
            timestamp: m.timestamp.to_string(),
        })
    }
}

#[derive(Clone)]
pub struct WalletRPC {
    pub wallet: Arc<Wallet>,
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
        info!(
            target: LOG_TARGET,
            "SendTextMessage gRPC Request received: {:?}", request,
        );

        let msg = request.into_inner();

        let response = match CommsPublicKey::from_base64(msg.dest_pub_key.as_str()) {
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
        info!(
            target: LOG_TARGET,
            "GetTextMessages gRPC Request received: {:?}", request
        );

        let response_body = match self.wallet.text_message_service.get_text_messages() {
            Ok(msgs) => sort_text_messages(msgs),
            _ => TextMessagesResponseRpc {
                sent_messages: Vec::new(),
                received_messages: Vec::new(),
            },
        };
        let response = Response::new(response_body);

        future::ok(response)
    }

    fn get_text_messages_by_contact(&mut self, request: Request<ContactRpc>) -> Self::GetTextMessagesFuture {
        info!(
            target: LOG_TARGET,
            "GetTextMessages gRPC Request received: {:?}", request
        );

        let msg = request.into_inner();

        let pub_key = match CommsPublicKey::from_base64(msg.pub_key.as_str()) {
            Ok(pk) => pk,
            _ => {
                return future::ok(Response::new(TextMessagesResponseRpc {
                    sent_messages: Vec::new(),
                    received_messages: Vec::new(),
                }))
            },
        };

        let response_body = match self.wallet.text_message_service.get_text_messages_by_pub_key(pub_key) {
            Ok(msgs) => sort_text_messages(msgs),
            _ => TextMessagesResponseRpc {
                sent_messages: Vec::new(),
                received_messages: Vec::new(),
            },
        };
        let response = Response::new(response_body);

        future::ok(response)
    }

    fn set_screen_name(&mut self, request: Request<ScreenNameRpc>) -> Self::SetScreenNameFuture {
        info!(target: LOG_TARGET, "SetScreenName gRPC Request received: {:?}", request,);

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
        info!(target: LOG_TARGET, "GetScreenName gRPC Request received: {:?}", request,);

        let _msg = request.into_inner();

        let screen_name = self
            .wallet
            .text_message_service
            .get_screen_name()
            .unwrap_or(Some("".to_string())) // Unwrap result
            .unwrap_or("".to_string()); // Unwrap Option

        future::ok(Response::new(ScreenNameRpc { screen_name }))
    }

    fn get_public_key(&mut self, request: Request<VoidParams>) -> Self::GetPublicKeyFuture {
        info!(target: LOG_TARGET, "GetPublicKey gRPC Request received: {:?}", request,);

        let _msg = request.into_inner();

        let public_key = self
            .wallet
            .public_key
            .clone()
            .to_base64()
            .unwrap_or("Failed to get public key".to_string());

        future::ok(Response::new(PublicKeyRpc { pub_key: public_key }))
    }

    fn add_contact(&mut self, request: Request<ContactRpc>) -> Self::AddContactFuture {
        info!(target: LOG_TARGET, "AddContact gRPC Request received: {:?}", request,);

        let msg = request.into_inner();

        let screen_name = msg.screen_name.clone();
        let pub_key = match CommsPublicKey::from_base64(msg.pub_key.as_str()) {
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
        }

        let response = match self.wallet.text_message_service.add_contact(Contact {
            screen_name,
            pub_key,
            address: net_address,
        }) {
            Ok(()) => Response::new(RpcResponse {
                success: true,
                message: "Successfully added contact".to_string(),
            }),
            Err(e) => Response::new(RpcResponse {
                success: false,
                message: format!("Error adding contact: {:?}", e).to_string(),
            }),
        };

        return future::ok(response);
    }

    fn remove_contact(&mut self, request: Request<ContactRpc>) -> Self::RemoveContactFuture {
        info!(target: LOG_TARGET, "RemoveContact gRPC Request received: {:?}", request,);

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

        if let Ok(pk) = CommsPublicKey::from_base64(msg.pub_key.as_str()) {
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
        info!(target: LOG_TARGET, "UpdateContact gRPC Request received: {:?}", request,);

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
        if let Ok(pk) = CommsPublicKey::from_base64(msg.pub_key.as_str()) {
            let response = match self.wallet.text_message_service.update_contact(pk, UpdateContact {
                screen_name,
                address: net_address,
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
        info!(target: LOG_TARGET, "GetContacts gRPC Request received: {:?}", request,);

        let mut contacts_resp: Vec<ContactRpc> = Vec::new();

        if let Ok(contacts) = self.wallet.text_message_service.get_contacts() {
            for c in contacts.iter() {
                let sn = c.screen_name.clone();
                let address = format!("{}", c.address.clone());
                if let Ok(pk) = c.pub_key.to_base64() {
                    contacts_resp.push(ContactRpc {
                        screen_name: sn,
                        pub_key: pk,
                        address,
                    });
                }
            }
        }

        future::ok(Response::new(ContactsRpc {
            contacts: contacts_resp,
        }))
    }
}

/// Utility function to sort out the interrim TextMessages Collection into an RPC response
pub fn sort_text_messages(msgs: TextMessages) -> TextMessagesResponseRpc {
    let mut body = TextMessagesResponseRpc {
        sent_messages: Vec::new(),
        received_messages: Vec::new(),
    };

    for m in msgs.received_messages {
        match ReceivedTextMessageRpc::try_from(m) {
            Ok(m) => body.received_messages.push(m),
            Err(e) => error!("Error serializing text messages: {:?}", e),
        }
    }

    // TODO TextMessageService will be refactored to use the boolean `acknowledged` flag rather than two
    // lists when the Sqlite backend is put in place
    for msg in msgs.pending_messages {
        let source = match msg.source_pub_key.to_base64() {
            Ok(pk) => pk,
            Err(e) => {
                error!(target: LOG_TARGET, "Error encoding Public Key to Base 64: {:?}", e);
                continue;
            },
        };
        let dest = match msg.dest_pub_key.to_base64() {
            Ok(pk) => pk,
            Err(e) => {
                error!(target: LOG_TARGET, "Error encoding Public Key to Base 64: {:?}", e);
                continue;
            },
        };
        body.sent_messages.push(SentTextMessageRpc {
            id: msg.id.to_base64().unwrap(),
            source_pub_key: source,
            dest_pub_key: dest,
            message: msg.message,
            timestamp: msg.timestamp.to_string(),
            acknowledged: false,
        });
    }

    for msg in msgs.sent_messages {
        let source = match msg.source_pub_key.to_base64() {
            Ok(pk) => pk,
            Err(e) => {
                error!(target: LOG_TARGET, "Error encoding Public Key to Base 64: {:?}", e);
                continue;
            },
        };
        let dest = match msg.dest_pub_key.to_base64() {
            Ok(pk) => pk,
            Err(e) => {
                error!(target: LOG_TARGET, "Error encoding Public Key to Base 64: {:?}", e);
                continue;
            },
        };
        body.sent_messages.push(SentTextMessageRpc {
            id: msg.id.to_base64().unwrap(),
            source_pub_key: source,
            dest_pub_key: dest,
            message: msg.message,
            timestamp: msg.timestamp.to_string(),
            acknowledged: true,
        });
    }
    body
}
