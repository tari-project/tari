//  Copyright 2022. The Tari Project
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

use std::convert::{TryFrom, TryInto};

use tari_common_types::{epoch::VnEpoch, types::CompressedPublicKey};
use tari_core::base_node::comms_interface::ValidatorNodeChange;
use tari_transaction_components::tari_amount::MicroMinotari;
use tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

// -------------------------------- ValidatorNodeChange -------------------------------- //

impl TryFrom<grpc::ValidatorNodeChange> for ValidatorNodeChange {
    type Error = String;

    fn try_from(value: grpc::ValidatorNodeChange) -> Result<Self, Self::Error> {
        let change = value.change.ok_or("change not provided")?;
        match change {
            grpc::validator_node_change::Change::Add(add) => {
                let activation_epoch = VnEpoch(add.activation_epoch);
                let registration = add.registration.ok_or("registration not provided")?.try_into()?;
                let minimum_value_promise = MicroMinotari(add.minimum_value_promise);
                if add.shard_key.len() != 32 {
                    return Err(format!("shard_key length is not 32 (len:{})", add.shard_key.len()));
                }
                let mut shard_key = [0u8; 32];
                shard_key.copy_from_slice(&add.shard_key);

                Ok(ValidatorNodeChange::Add {
                    registration: Box::new(registration),
                    activation_epoch,
                    minimum_value_promise,
                    shard_key,
                })
            },
            grpc::validator_node_change::Change::Remove(remove) => {
                let public_key =
                    CompressedPublicKey::from_canonical_bytes(&remove.public_key).map_err(|e| e.to_string())?;
                Ok(ValidatorNodeChange::Remove { public_key })
            },
        }
    }
}

impl From<&ValidatorNodeChange> for grpc::ValidatorNodeChange {
    fn from(node_change: &ValidatorNodeChange) -> Self {
        match node_change {
            ValidatorNodeChange::Add {
                registration,
                activation_epoch,
                minimum_value_promise,
                shard_key,
            } => Self {
                change: Some(grpc::validator_node_change::Change::Add(grpc::ValidatorNodeChangeAdd {
                    activation_epoch: activation_epoch.as_u64(),
                    registration: Some((&**registration).into()),
                    minimum_value_promise: (*minimum_value_promise).into(),
                    shard_key: shard_key.to_vec(),
                })),
            },
            ValidatorNodeChange::Remove { public_key } => Self {
                change: Some(grpc::validator_node_change::Change::Remove(
                    grpc::ValidatorNodeChangeRemove {
                        public_key: public_key.to_vec(),
                    },
                )),
            },
        }
    }
}
