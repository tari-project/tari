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

//! # LibWallet API Definition
//! This module contains the Rust backend implementations of the functionality that a wallet for the Tari Base Layer
//! will require. The module contains a number of sub-modules that are implemented as async services. These services are
//! collected into the main Wallet container struct which manages spinning up all the component services and maintains a
//! collection of the handles required to interact with those services.
//! This files contians the API calls that will be exposed to external systems that make use of this module. The API
//! will be exposed via FFI and will consist of API calls that the FFI client can make into the Wallet module and a set
//! of Callbacks that the client must implement and provide to the Wallet module to receive asynchronous replies and
//! updates.

// TODO: Improve documentation

#ifndef wallet_ffi_h
#define wallet_ffi_h

#include <stdio.h>
#include <stdbool.h>

struct ByteVector;

struct TariCommsConfig;

struct TariPrivateKey;

struct TariWallet;

struct TariWalletConfig;

struct TariPublicKey;

struct TariContacts;

struct TariContact;

struct TariCompletedTransactions;

struct TariCompletedTransaction;

struct TariPendingOutboundTransactions;

struct TariPendingOutboundTransaction;

struct TariPendingInboundTransactions;

struct TariPendingInboundTransaction;


/// -------------------------------- Strings ----------------------------------------------- ///

// Frees memory for a string pointer
void string_destroy(char *s);

/// -------------------------------- ByteVector ----------------------------------------------- ///

// Creates a ByteVector
struct ByteVector *byte_vector_create(const unsigned char *byte_array, unsigned int element_count);

// Gets a char from a ByteVector
unsigned char byte_vector_get_at(struct ByteVector *ptr, unsigned int i);

// Returns the number of elements in a ByteVector
unsigned int byte_vector_get_length(const struct ByteVector *vec);

// Frees memory for a ByteVector pointer
void byte_vector_destroy(struct ByteVector *bytes);

/// -------------------------------- TariPublicKey ----------------------------------------------- ///

// Creates a TariPublicKey from a ByteVector
struct TariPublicKey *public_key_create(struct ByteVector *bytes);

// Gets a ByteVector from a TariPublicKey
struct ByteVector *public_key_get_bytes(struct TariPublicKey *public_key);

// Creates a TariPublicKey from a TariPrivateKey
struct TariPublicKey *public_key_get_from_private_key(struct TariPrivateKey *secret_key);

// Creates a TariPublicKey from a const char* filled with hexadecimal characters
struct TariPublicKey *public_key_from_hex(const char *hex);

// Frees memory for a TariPublicKey pointer
void public_key_destroy(struct TariPublicKey *pk);

/// -------------------------------- TariPrivateKey ----------------------------------------------- ///

// Creates a TariPrivateKey from a ByteVector
struct TariPrivateKey *private_key_create(struct ByteVector *bytes);

// Generates a TariPrivateKey
struct TariPrivateKey *private_key_generate(void);

// Creates a ByteVector from a TariPrivateKey
struct ByteVector *private_key_get_bytes(struct TariPrivateKey *private_key);

// Creates a TariPrivateKey from a const char* filled with hexadecimal charaters
struct TariPrivateKey *private_key_from_hex(const char *hex);

// Frees memory for a TariPrivateKey
void private_key_destroy(struct TariPrivateKey *pk);

/// -------------------------------- Contact ------------------------------------------------------ ///

// Creates a TariContact
struct TariContact *contact_create(const char *alias, struct TariPublicKey *public_key);

// Gets the alias of the TariContact
char *contact_get_alias(struct TariContact *contact);

/// Gets the TariPublicKey of the TariContact
struct TariPublicKey *contact_get_public_key(struct TariContact *contact);

// Frees memory for a TariContact
void contact_destroy(struct TariContact *contact);

/// -------------------------------- Contacts ------------------------------------------------------ ///

// Gets the number of elements of TariContacts
unsigned int contacts_get_length(struct TariContacts *contacts);

// Gets a TariContact from TariContacts at position
struct TariContact *contacts_get_at(struct TariContacts *contacts, unsigned int position);

// Frees memory for TariContacts
void contacts_destroy(struct TariContacts *contacts);

/// -------------------------------- CompletedTransaction ------------------------------------------------------ ///

// Gets a transaction id of a TariCompletedTransaction
unsigned long long completed_transaction_get_transaction_id(struct TariCompletedTransaction *transaction);

// Gets the destination TariPublicKey of a TariCompletedTransaction
struct TariPublicKey *completed_transaction_get_destination_public_key(struct TariCompletedTransaction *transaction);

// Gets the source TariPublicKey of a TariCompletedTransaction
struct TariPublicKey *completed_transaction_get_source_public_key(struct TariCompletedTransaction *transaction);

// Gets the amount of a TariCompletedTransaction
unsigned long long completed_transaction_get_amount(struct TariCompletedTransaction *transaction);

// Gets the fee of a TariCompletedTransaction
unsigned long long completed_transaction_get_fee(struct TariCompletedTransaction *transaction);

unsigned long long completed_transaction_get_timestamp(struct TariCompletedTransaction *transaction);

// Gets the message of a TariCompletedTransaction
const char *completed_transaction_get_message(struct TariCompletedTransaction *transaction);

// Gets the status of a TariCompletedTransaction
// | Value | Interpretation |
// |---|---|
// |  -1 | TxNullError |
// |   0 | Completed |
// |   1 | Broadcast |
// |   2 | Mined |
char completed_transaction_get_status(struct TariCompletedTransaction *transaction);

// Gets the TransactionID of a TariCompletedTransaction
unsigned long long completed_transaction_get_transaction_id(struct TariCompletedTransaction *transaction);

// Gets the timestamp of a TariCompletedTransaction
unsigned long long completed_transaction_get_transaction_timestamp(struct TariCompletedTransaction *transaction);

// Frees memory for a TariCompletedTransaction
void completed_transaction_destroy(struct TariCompletedTransaction *transaction);

/// -------------------------------- CompletedTransactions ------------------------------------------------------ ///

// Gets number of elements in TariCompletedTransactions
unsigned int completed_transactions_get_length(struct TariCompletedTransactions *transactions);

// Gets a TariCompletedTransaction from a TariCompletedTransactions at position
struct TariCompletedTransaction *completed_transactions_get_at(struct TariCompletedTransactions *transactions, unsigned int position);

// Frees memory for a TariCompletedTransactions
void completed_transactions_destroy(struct TariCompletedTransactions *transactions);

/// -------------------------------- OutboundTransaction ------------------------------------------------------ ///

// Gets the TransactionId of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_transaction_id(struct TariPendingOutboundTransaction *transaction);

// Gets the destination TariPublicKey of a TariPendingOutboundTransaction
struct TariPublicKey *pending_outbound_transaction_get_destination_public_key(struct TariPendingOutboundTransaction *transaction);

// Gets the amount of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_amount(struct TariPendingOutboundTransaction *transaction);

// Gets the message of a TariPendingOutboundTransaction
const char *pending_outbound_transaction_get_message(struct TariPendingOutboundTransaction *transaction);

// Gets the timestamp of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_timestamp(struct TariPendingOutboundTransaction *transaction);

// Frees memory for a TariPendingOutboundTactions
void pending_inbound_transaction_destroy(struct TariPendingInboundTransaction *transaction);

/// -------------------------------- OutboundTransactions ------------------------------------------------------ ///

// Gets the number of elements in a TariPendingOutboundTactions
unsigned int pending_outbound_transactions_get_length(struct TariPendingOutboundTransactions *transactions);

// Gets a TariPendingOutboundTransaction of a TariPendingOutboundTransactions at position
struct TariPendingOutboundTransactions *pending_outbound_transactions_get_at(struct TariPendingOutboundTransactions *transactions, unsigned int position);

// Frees memory of a TariPendingOutboundTransactions
void pending_outbound_transactions_destroy(struct TariPendingOutboundTransactions *transactions);

/// -------------------------------- InboundTransaction ------------------------------------------------------ ///

// Gets the TransactionId of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_transaction_id(struct TariPendingInboundTransaction *transaction);

// Gets the source TariPublicKey of a TariPendingInboundTransaction
struct TariPublicKey *pending_inbound_transaction_get_source_public_key(struct TariPendingInboundTransaction *transaction);

// Gets the message of a TariPendingInboundTransaction
const char *pending_inbound_transaction_get_message(struct TariPendingInboundTransaction *transaction);

// Gets the amount of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_amount(struct TariPendingInboundTransaction *transaction);

// Gets the timestamp of a TariPendingInboundTransaction
unsigned long long pending_inbound_get_timestamp(struct TariPendingInboundTransaction *transaction);

// Frees memory for a TariPendingInboundTransaction
void pending_inbound_transaction_destroy(struct TariPendingInboundTransaction *transaction);

/// -------------------------------- InboundTransactions ------------------------------------------------------ ///

// Gets the number of elements in a TariPendingInboundTransactions
unsigned int pending_inbound_transactions_get_length(struct TariPendingInboundTransactions *transactions);

// Gets a TariPendingInboundTransaction of a TariPendingInboundTransactions at position
struct TariPendingInboundTransactions *pending_inbound_transactions_get_at(struct TariPendingInboundTransactions *transactions, unsigned int position);

// Frees memory of a TariPendingInboundTransaction
void pending_inbound_transactions_destroy(struct TariPendingInboundTransactions *transactions);

/// -------------------------------- TariCommsConfig ----------------------------------------------- ///

// Creates a TariCommsConfig
struct TariCommsConfig *comms_config_create(char *address,
                                     char *database_name,
                                     char *datastore_path,
                                     struct TariPrivateKey *secret_key);

// Frees memory for a TariCommsConfig
void comms_config_destroy(struct TariCommsConfig *wc);

/// -------------------------------- TariWallet ----------------------------------------------- //

// Creates a TariWallet
struct TariWallet *wallet_create(struct TariWalletConfig *config);

/// Generates test data
bool wallet_generate_test_data(struct TariWallet *wallet);

// Adds a base node peer to the TariWallet
bool wallet_add_base_node_peer(struct TariWallet *wallet, struct TariPublicKey *public_key, char *address);

// Adds a TariContact to the TariWallet
bool wallet_add_contact(struct TariWallet *wallet, struct TariContact *contact);

// Removes a TariContact form the TariWallet
bool wallet_remove_contact(struct TariWallet *wallet, struct TariContact *contact);

// Gets the available balance from a TariWallet
unsigned long long wallet_get_available_balance(struct TariWallet *wallet);

// Gets the incoming balance from a TariWallet
unsigned long long wallet_get_incoming_balance(struct TariWallet *wallet);

// Gets the outgoing balance from a TariWallet
unsigned long long wallet_get_outgoing_balance(struct TariWallet *wallet);

// Sends a TariPendingOutboundTransaction
bool wallet_send_transaction(struct TariWallet *wallet, struct TariPublicKey *destination, unsigned long long amount, unsigned long long fee_per_gram,const char *message);

// Get the TariContacts from a TariWallet
struct TariContacts *wallet_get_contacts(struct TariWallet *wallet);

// Get the TariCompletedTransactions from a TariWallet
struct TariCompletedTransactions *wallet_get_completed_transactions(struct TariWallet *wallet);

// Get the TariPendingOutboundTransactions from a TariWallet
struct TariPendingOutboundTransactions *wallet_get_pending_outbound_transactions(struct TariWallet *wallet);

// Get the TariPublicKey from a TariCommsConfig
struct TariPublicKey *wallet_get_public_key(struct TariCommsConfig *wc);

// Get the TariPendingInboundTransactions from a TariWallet
struct TariPendingInboundTransactions *wallet_get_pending_inbound_transactions(struct TariWallet *wallet);

// Get the TariCompletedTransaction from a TariWallet by its TransactionId
struct TariCompletedTransaction *wallet_get_completed_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id);

// Get the TariPendingOutboundTransaction from a TariWallet by its TransactionId
struct TariPendingOutboundTransaction *wallet_get_pending_outbound_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id);

// Get the TariPendingInboundTransaction from a TariWallet by its TransactionId
struct TariPendingInboundTransaction *wallet_get_pending_inbound_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id);

// Simulates completion of a TariPendingOutboundTransaction
bool wallet_test_complete_sent_transaction(struct TariWallet *wallet, struct TariPendingOutboundTransaction *tx);

// Simulates the completion of a broadcasted TariPendingInboundTransaction
bool wallet_test_transaction_broadcast(struct TariWallet *wallet, struct TariPendingInboundTransaction *tx);

// Simulates a TariCompletedTransaction that has been mined
bool wallet_test_mined(struct TariWallet *wallet, struct TariCompletedTransaction *tx);

// Simulates a TariPendingInboundtransaction being received
bool wallet_test_receive_transaction(struct TariWallet *wallet);

// Frees memory for a TariWallet
void wallet_destroy(struct TariWallet *wallet);

// Registers a callback function for when TariPendingInboundTransaction broadcast is detected
bool wallet_callback_register_transaction_broadcast(struct TariWallet *wallet, void (*call)(struct TariCompletedTransaction*));

// Registers a callback function for when a TariCompletedTransaction is mined
bool wallet_callback_register_mined(struct TariWallet *wallet, void (*call)(struct TariCompletedTransaction*));

// Registers a callback function for when a TariPendingInboundTransaction is received
bool wallet_callback_register_received_transaction(struct TariWallet *wallet, void (*call)(struct TariPendingInboundTransaction*));

// Registers a callback function for when a reply is received for a TariPendingOutboundTransaction
bool wallet_callback_register_received_transaction_reply(struct TariWallet *wallet, void (*call)(struct TariCompletedTransaction*));


#endif /* wallet_ffi_h */