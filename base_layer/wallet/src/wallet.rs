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
    contacts_service::{handle::ContactsServiceHandle, storage::database::ContactsBackend, ContactsServiceInitializer},
    error::WalletError,
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::OutputManagerHandle,
        protocols::utxo_validation_protocol::UtxoValidationRetry,
        storage::database::OutputManagerBackend,
        OutputManagerServiceInitializer,
        TxId,
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionServiceHandle,
        storage::database::TransactionBackend,
        TransactionServiceInitializer,
    },
};
use blake2::Digest;
use log::*;
use std::{marker::PhantomData, sync::Arc};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
    CommsNode,
};
use tari_comms_dht::{store_forward::StoreAndForwardRequester, Dht};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{OutputFeatures, UnblindedOutput},
    types::{CryptoFactories, PrivateKey},
};
use tari_crypto::{
    common::Blake256,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    signatures::{SchnorrSignature, SchnorrSignatureError},
    tari_utilities::hex::Hex,
};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{initialize_comms, CommsConfig},
    services::comms_outbound::CommsOutboundServiceInitializer,
};
use tari_service_framework::StackBuilder;
use tokio::runtime::Runtime;

const LOG_TARGET: &str = "wallet";

#[derive(Clone)]
pub struct WalletConfig {
    pub comms_config: CommsConfig,
    pub factories: CryptoFactories,
    pub transaction_service_config: Option<TransactionServiceConfig>,
}

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
pub struct Wallet<T, U, V, W>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + Clone + 'static,
    V: OutputManagerBackend + Clone + 'static,
    W: ContactsBackend + 'static,
{
    pub comms: CommsNode,
    pub dht_service: Dht,
    pub store_and_forward_requester: StoreAndForwardRequester,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_service: TransactionServiceHandle,
    pub contacts_service: ContactsServiceHandle,
    pub db: WalletDatabase<T>,
    pub runtime: Runtime,
    pub factories: CryptoFactories,
    #[cfg(feature = "test_harness")]
    pub transaction_backend: U,
    _u: PhantomData<U>,
    _v: PhantomData<V>,
    _w: PhantomData<W>,
}

impl<T, U, V, W> Wallet<T, U, V, W>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + Clone + 'static,
    V: OutputManagerBackend + Clone + 'static,
    W: ContactsBackend + 'static,
{
    pub fn new(
        config: WalletConfig,
        mut runtime: Runtime,
        wallet_backend: T,
        transaction_backend: U,
        output_manager_backend: V,
        contacts_backend: W,
    ) -> Result<Wallet<T, U, V, W>, WalletError>
    {
        let database_path = config
            .comms_config
            .datastore_path
            .join(config.comms_config.peer_database_name.clone())
            .with_extension("sqlite3");
        let db = WalletDatabase::new(wallet_backend, Some(database_path));
        let base_node_peers = runtime.block_on(db.get_peers())?;

        #[cfg(feature = "test_harness")]
        let transaction_backend_handle = transaction_backend.clone();

        let factories = config.factories;
        let (publisher, subscription_factory) = pubsub_connector(runtime.handle().clone(), 1500);
        let subscription_factory = Arc::new(subscription_factory);

        let (comms, dht) = runtime.block_on(initialize_comms(
            config.comms_config.clone(),
            publisher,
            vec![],
            Default::default(),
        ))?;

        let fut = StackBuilder::new(runtime.handle().clone(), comms.shutdown_signal())
            .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
            .add_initializer(OutputManagerServiceInitializer::new(
                OutputManagerServiceConfig::default(),
                subscription_factory.clone(),
                output_manager_backend,
                factories.clone(),
            ))
            .add_initializer(TransactionServiceInitializer::new(
                config.transaction_service_config.unwrap_or_default(),
                subscription_factory,
                transaction_backend,
                comms.node_identity(),
                factories.clone(),
            ))
            .add_initializer(ContactsServiceInitializer::new(contacts_backend))
            .finish();

        let handles = runtime.block_on(fut).expect("Service initialization failed");

        let mut output_manager_handle = handles
            .get_handle::<OutputManagerHandle>()
            .expect("Could not get Output Manager Service Handle");
        let mut transaction_service_handle = handles
            .get_handle::<TransactionServiceHandle>()
            .expect("Could not get Transaction Service Handle");
        let contacts_handle = handles
            .get_handle::<ContactsServiceHandle>()
            .expect("Could not get Contacts Service Handle");

        for p in base_node_peers {
            runtime.block_on(transaction_service_handle.set_base_node_public_key(p.public_key.clone()))?;
            runtime.block_on(output_manager_handle.set_base_node_public_key(p.public_key.clone()))?;
        }

        let store_and_forward_requester = dht.store_and_forward_requester();

        Ok(Wallet {
            comms,
            dht_service: dht,
            store_and_forward_requester,
            output_manager_service: output_manager_handle,
            transaction_service: transaction_service_handle,
            contacts_service: contacts_handle,
            db,
            runtime,
            factories,
            #[cfg(feature = "test_harness")]
            transaction_backend: transaction_backend_handle,
            _u: PhantomData,
            _v: PhantomData,
            _w: PhantomData,
        })
    }

    /// This method consumes the wallet so that the handles are dropped which will result in the services async loops
    /// exiting.
    pub fn shutdown(mut self) {
        self.runtime.block_on(self.comms.shutdown());
    }

    /// This function will set the base_node that the wallet uses to broadcast transactions and monitor the blockchain
    /// state
    pub fn set_base_node_peer(&mut self, public_key: CommsPublicKey, net_address: String) -> Result<(), WalletError> {
        let address = net_address.parse::<Multiaddr>()?;
        let peer = Peer::new(
            public_key.clone(),
            NodeId::from_key(&public_key).unwrap(),
            vec![address].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            &[],
        );

        let existing_peers = self.runtime.block_on(self.db.get_peers())?;
        // Remove any peers in db to only persist a single peer at a time.
        for p in existing_peers {
            let _ = self.runtime.block_on(self.db.remove_peer(p.public_key.clone()))?;
        }
        self.runtime.block_on(self.db.save_peer(peer.clone()))?;

        self.runtime
            .block_on(self.comms.peer_manager().add_peer(peer.clone()))?;
        self.runtime
            .block_on(self.comms.connectivity().add_managed_peers(vec![peer.node_id.clone()]))?;
        self.runtime.block_on(
            self.transaction_service
                .set_base_node_public_key(peer.public_key.clone()),
        )?;
        self.runtime
            .block_on(self.output_manager_service.set_base_node_public_key(peer.public_key))?;

        Ok(())
    }

    /// Import an external spendable UTXO into the wallet. The output will be added to the Output Manager and made
    /// spendable. A faux incoming transaction will be created to provide a record of the event. The TxId of the
    /// generated transaction is returned.
    pub fn import_utxo(
        &mut self,
        amount: MicroTari,
        spending_key: &PrivateKey,
        source_public_key: &CommsPublicKey,
        message: String,
    ) -> Result<TxId, WalletError>
    {
        let unblinded_output = UnblindedOutput::new(amount, spending_key.clone(), None);

        self.runtime
            .block_on(self.output_manager_service.add_output(unblinded_output.clone()))?;

        let tx_id = self.runtime.block_on(self.transaction_service.import_utxo(
            amount,
            source_public_key.clone(),
            message,
        ))?;

        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}) imported into wallet",
            unblinded_output
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default())
                .commitment
                .to_hex()
        );

        Ok(tx_id)
    }

    pub fn sign_message(
        &mut self,
        secret: RistrettoSecretKey,
        nonce: RistrettoSecretKey,
        message: &str,
    ) -> Result<SchnorrSignature<RistrettoPublicKey, RistrettoSecretKey>, SchnorrSignatureError>
    {
        let challenge = Blake256::digest(message.as_bytes());
        RistrettoSchnorr::sign(secret, nonce, challenge.clone().as_slice())
    }

    pub fn verify_message_signature(
        &mut self,
        public_key: RistrettoPublicKey,
        public_nonce: RistrettoPublicKey,
        signature: RistrettoSecretKey,
        message: String,
    ) -> bool
    {
        let signature = RistrettoSchnorr::new(public_nonce, signature);
        let challenge = Blake256::digest(message.as_bytes());
        signature.verify_challenge(&public_key, challenge.clone().as_slice())
    }

    /// Have all the wallet components that need to start a sync process with the set base node to confirm the wallets
    /// state is accurately reflected on the blockchain
    pub fn validate_utxos(&mut self, retries: UtxoValidationRetry) -> Result<u64, WalletError> {
        self.runtime
            .block_on(self.store_and_forward_requester.request_saf_messages_from_neighbours())?;

        let request_key = self
            .runtime
            .block_on(self.output_manager_service.validate_utxos(retries))?;
        Ok(request_key)
    }

    /// Do a coin split
    pub fn coin_split(
        &mut self,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
        message: String,
        lock_height: Option<u64>,
    ) -> Result<TxId, WalletError>
    {
        let coin_split_tx = self.runtime.block_on(self.output_manager_service.create_coin_split(
            amount_per_split,
            split_count,
            fee_per_gram,
            lock_height,
        ));

        match coin_split_tx {
            Ok((tx_id, split_tx, amount, fee)) => {
                let coin_tx = self.runtime.block_on(
                    self.transaction_service
                        .submit_transaction(tx_id, split_tx, fee, amount, message),
                );
                match coin_tx {
                    Ok(_) => Ok(tx_id),
                    Err(e) => Err(WalletError::TransactionServiceError(e)),
                }
            },
            Err(e) => Err(WalletError::OutputManagerError(e)),
        }
    }
}
