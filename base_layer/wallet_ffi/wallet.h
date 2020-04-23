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

#ifdef __cplusplus
extern "C" {
#endif

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

struct TariTransportType;

/// -------------------------------- Transport Types ----------------------------------------------- ///

// Creates a memory transport type
struct TariTransportType *transport_memory_create();

// Creates a tcp transport type
struct TariTransportType *transport_tcp_create(const char *listener_address,int* error_out);

// Creates a tor transport type
struct TariTransportType *transport_tor_create(
    const char *control_server_address,
    struct ByteVector *tor_cookie,
    struct ByteVector *tor_identity,
    unsigned short tor_port,
    const char *socks_username,
    const char *socks_password,
    int* error_out);

// Gets the tor private key from the wallet
struct ByteVector *wallet_get_tor_identity(struct TariWallet *wallet,int* error_out );

// Gets the address from a memory transport type
char *transport_memory_get_address(struct TariTransportType *transport,int* error_out);

// Frees memory for a transport type
void transport_type_destroy(struct TariTransportType *transport);

/// -------------------------------- Strings ----------------------------------------------- ///

// Frees memory for a string pointer
void string_destroy(char *s);

/// -------------------------------- ByteVector ----------------------------------------------- ///

// Creates a ByteVector
struct ByteVector *byte_vector_create(const unsigned char *byte_array, unsigned int element_count, int* error_out);

// Gets a char from a ByteVector
unsigned char byte_vector_get_at(struct ByteVector *ptr, unsigned int i, int* error_out);

// Returns the number of elements in a ByteVector
unsigned int byte_vector_get_length(const struct ByteVector *vec, int* error_out);

// Frees memory for a ByteVector pointer
void byte_vector_destroy(struct ByteVector *bytes);

/// -------------------------------- TariPublicKey ----------------------------------------------- ///

// Creates a TariPublicKey from a ByteVector
struct TariPublicKey *public_key_create(struct ByteVector *bytes,int* error_out);

// Gets a ByteVector from a TariPublicKey
struct ByteVector *public_key_get_bytes(struct TariPublicKey *public_key,int* error_out);

// Creates a TariPublicKey from a TariPrivateKey
struct TariPublicKey *public_key_from_private_key(struct TariPrivateKey *secret_key,int* error_out);

// Creates a TariPublicKey from a const char* filled with hexadecimal characters
struct TariPublicKey *public_key_from_hex(const char *hex,int* error_out);

// Frees memory for a TariPublicKey pointer
void public_key_destroy(struct TariPublicKey *pk);

//Converts a TariPublicKey to char array in emoji format
char *public_key_to_emoji_id(struct TariPublicKey *pk, int* error_out);

// Converts a char array in emoji format to a public key
struct TariPublicKey *emoji_id_to_public_key(const char *emoji,  int* error_out);

/// -------------------------------- TariPrivateKey ----------------------------------------------- ///

// Creates a TariPrivateKey from a ByteVector
struct TariPrivateKey *private_key_create(struct ByteVector *bytes,int* error_out);

// Generates a TariPrivateKey
struct TariPrivateKey *private_key_generate(void);

// Creates a ByteVector from a TariPrivateKey
struct ByteVector *private_key_get_bytes(struct TariPrivateKey *private_key,int* error_out);

// Creates a TariPrivateKey from a const char* filled with hexadecimal characters
struct TariPrivateKey *private_key_from_hex(const char *hex,int* error_out);

// Frees memory for a TariPrivateKey
void private_key_destroy(struct TariPrivateKey *pk);

/// -------------------------------- Contact ------------------------------------------------------ ///

// Creates a TariContact
struct TariContact *contact_create(const char *alias, struct TariPublicKey *public_key,int* error_out);

// Gets the alias of the TariContact
char *contact_get_alias(struct TariContact *contact,int* error_out);

/// Gets the TariPublicKey of the TariContact
struct TariPublicKey *contact_get_public_key(struct TariContact *contact,int* error_out);

// Frees memory for a TariContact
void contact_destroy(struct TariContact *contact);

/// -------------------------------- Contacts ------------------------------------------------------ ///

// Gets the number of elements of TariContacts
unsigned int contacts_get_length(struct TariContacts *contacts,int* error_out);

// Gets a TariContact from TariContacts at position
struct TariContact *contacts_get_at(struct TariContacts *contacts, unsigned int position,int* error_out);

// Frees memory for TariContacts
void contacts_destroy(struct TariContacts *contacts);

/// -------------------------------- CompletedTransaction ------------------------------------------------------ ///

// Gets the destination TariPublicKey of a TariCompletedTransaction
struct TariPublicKey *completed_transaction_get_destination_public_key(struct TariCompletedTransaction *transaction,int* error_out);

// Gets the source TariPublicKey of a TariCompletedTransaction
struct TariPublicKey *completed_transaction_get_source_public_key(struct TariCompletedTransaction *transaction,int* error_out);

// Gets the amount of a TariCompletedTransaction
unsigned long long completed_transaction_get_amount(struct TariCompletedTransaction *transaction,int* error_out);

// Gets the fee of a TariCompletedTransaction
unsigned long long completed_transaction_get_fee(struct TariCompletedTransaction *transaction,int* error_out);

// Gets the message of a TariCompletedTransaction
const char *completed_transaction_get_message(struct TariCompletedTransaction *transaction,int* error_out);

// Gets the status of a TariCompletedTransaction
// | Value | Interpretation |
// |---|---|
// |  -1 | TxNullError |
// |   0 | Completed   |
// |   1 | Broadcast   |
// |   2 | Mined       |
// |   3 | Imported    |
// |   4 | Pending     |
int completed_transaction_get_status(struct TariCompletedTransaction *transaction,int* error_out);

// Gets the TransactionID of a TariCompletedTransaction
unsigned long long completed_transaction_get_transaction_id(struct TariCompletedTransaction *transaction,int* error_out);

// Gets the timestamp of a TariCompletedTransaction
unsigned long long completed_transaction_get_timestamp(struct TariCompletedTransaction *transaction,int* error_out);

// Frees memory for a TariCompletedTransaction
void completed_transaction_destroy(struct TariCompletedTransaction *transaction);

/// -------------------------------- CompletedTransactions ------------------------------------------------------ ///

// Gets number of elements in TariCompletedTransactions
unsigned int completed_transactions_get_length(struct TariCompletedTransactions *transactions,int* error_out);

// Gets a TariCompletedTransaction from a TariCompletedTransactions at position
struct TariCompletedTransaction *completed_transactions_get_at(struct TariCompletedTransactions *transactions, unsigned int position,int* error_out);

// Frees memory for a TariCompletedTransactions
void completed_transactions_destroy(struct TariCompletedTransactions *transactions);

/// -------------------------------- OutboundTransaction ------------------------------------------------------ ///

// Gets the TransactionId of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_transaction_id(struct TariPendingOutboundTransaction *transaction,int* error_out);

// Gets the destination TariPublicKey of a TariPendingOutboundTransaction
struct TariPublicKey *pending_outbound_transaction_get_destination_public_key(struct TariPendingOutboundTransaction *transaction,int* error_out);

// Gets the amount of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_amount(struct TariPendingOutboundTransaction *transaction,int* error_out);

// Gets the fee of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_fee(struct TariPendingOutboundTransaction *transaction,int* error_out);

// Gets the message of a TariPendingOutboundTransaction
const char *pending_outbound_transaction_get_message(struct TariPendingOutboundTransaction *transaction,int* error_out);

// Gets the timestamp of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_timestamp(struct TariPendingOutboundTransaction *transaction,int* error_out);

// Gets the status of a TariPendingOutboundTransaction
// | Value | Interpretation |
// |---|---|
// |  -1 | TxNullError |
// |   0 | Completed   |
// |   1 | Broadcast   |
// |   2 | Mined       |
// |   3 | Imported    |
// |   4 | Pending     |
int pending_outbound_transaction_get_status(struct TariPendingOutboundTransaction *transaction,int* error_out);

// Frees memory for a TariPendingOutboundTactions
void pending_outbound_transaction_destroy(struct TariPendingOutboundTransaction *transaction);

/// -------------------------------- OutboundTransactions ------------------------------------------------------ ///

// Gets the number of elements in a TariPendingOutboundTactions
unsigned int pending_outbound_transactions_get_length(struct TariPendingOutboundTransactions *transactions,int* error_out);

// Gets a TariPendingOutboundTransaction of a TariPendingOutboundTransactions at position
struct TariPendingOutboundTransaction *pending_outbound_transactions_get_at(struct TariPendingOutboundTransactions *transactions, unsigned int position,int* error_out);

// Frees memory of a TariPendingOutboundTransactions
void pending_outbound_transactions_destroy(struct TariPendingOutboundTransactions *transactions);

/// -------------------------------- InboundTransaction ------------------------------------------------------ ///

// Gets the TransactionId of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_transaction_id(struct TariPendingInboundTransaction *transaction,int* error_out);

// Gets the source TariPublicKey of a TariPendingInboundTransaction
struct TariPublicKey *pending_inbound_transaction_get_source_public_key(struct TariPendingInboundTransaction *transaction,int* error_out);

// Gets the message of a TariPendingInboundTransaction
const char *pending_inbound_transaction_get_message(struct TariPendingInboundTransaction *transaction,int* error_out);

// Gets the amount of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_amount(struct TariPendingInboundTransaction *transaction,int* error_out);

// Gets the timestamp of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_timestamp(struct TariPendingInboundTransaction *transaction,int* error_out);

// Gets the status of a TariPendingInboundTransaction
// | Value | Interpretation |
// |---|---|
// |  -1 | TxNullError |
// |   0 | Completed   |
// |   1 | Broadcast   |
// |   2 | Mined       |
// |   3 | Imported    |
// |   4 | Pending     |
int pending_inbound_transaction_get_status(struct TariPendingInboundTransaction *transaction,int* error_out);

// Frees memory for a TariPendingInboundTransaction
void pending_inbound_transaction_destroy(struct TariPendingInboundTransaction *transaction);

/// -------------------------------- InboundTransactions ------------------------------------------------------ ///

// Gets the number of elements in a TariPendingInboundTransactions
unsigned int pending_inbound_transactions_get_length(struct TariPendingInboundTransactions *transactions,int* error_out);

// Gets a TariPendingInboundTransaction of a TariPendingInboundTransactions at position
struct TariPendingInboundTransaction *pending_inbound_transactions_get_at(struct TariPendingInboundTransactions *transactions, unsigned int position,int* error_out);

// Frees memory of a TariPendingInboundTransaction
void pending_inbound_transactions_destroy(struct TariPendingInboundTransactions *transactions);

/// -------------------------------- TariCommsConfig ----------------------------------------------- ///
// Creates a TariCommsConfig
struct TariCommsConfig *comms_config_create(const char *public_address,
                                     struct TariTransportType *transport,
                                     const char *database_name,
                                     const char *datastore_path,
                                     struct TariPrivateKey *secret_key,
                                     unsigned long long discovery_timeout_in_secs,
                                     int* error_out);

// Frees memory for a TariCommsConfig
void comms_config_destroy(struct TariCommsConfig *wc);

/// -------------------------------- TariWallet ----------------------------------------------- //

// Creates a TariWallet
struct TariWallet *wallet_create(struct TariWalletConfig *config,
                                    const char *log_path,
                                    void (*callback_received_transaction)(struct TariPendingInboundTransaction*),
                                    void (*callback_received_transaction_reply)(struct TariCompletedTransaction*),
                                    void (*callback_received_finalized_transaction)(struct TariCompletedTransaction*),
                                    void (*callback_transaction_broadcast)(struct TariCompletedTransaction*),
                                    void (*callback_transaction_mined)(struct TariCompletedTransaction*),
                                    void (*callback_direct_send_result)(unsigned long long, bool),
                                    void (*callback_store_and_forward_send_result)(unsigned long long, bool),
                                    void (*callback_transaction_cancellation)(unsigned long long),
                                    void (*callback_base_node_sync_complete)(unsigned long long, bool),
                                    int* error_out);

// Signs a message
char* wallet_sign_message(struct TariWallet *wallet, const char* msg, int* error_out);

// Verifies signature for a signed message
bool wallet_verify_message_signature(struct TariWallet *wallet, struct TariPublicKey *public_key, const char* hex_sig_nonce, const char* msg, int* error_out);

/// Generates test data
bool wallet_test_generate_data(struct TariWallet *wallet, const char *datastore_path,int* error_out);

// Adds a base node peer to the TariWallet
bool wallet_add_base_node_peer(struct TariWallet *wallet, struct TariPublicKey *public_key, const char *address,int* error_out);

// Upserts a TariContact to the TariWallet, if the contact does not exist it is inserted and if it does the alias is updated
bool wallet_upsert_contact(struct TariWallet *wallet, struct TariContact *contact,int* error_out);

// Removes a TariContact form the TariWallet
bool wallet_remove_contact(struct TariWallet *wallet, struct TariContact *contact,int* error_out);

// Gets the available balance from a TariWallet
unsigned long long wallet_get_available_balance(struct TariWallet *wallet,int* error_out);

// Gets the incoming balance from a TariWallet
unsigned long long wallet_get_pending_incoming_balance(struct TariWallet *wallet,int* error_out);

// Gets the outgoing balance from a TariWallet
unsigned long long wallet_get_pending_outgoing_balance(struct TariWallet *wallet,int* error_out);

// Sends a TariPendingOutboundTransaction
unsigned long long wallet_send_transaction(struct TariWallet *wallet, struct TariPublicKey *destination, unsigned long long amount, unsigned long long fee_per_gram,const char *message,int* error_out);

// Get the TariContacts from a TariWallet
struct TariContacts *wallet_get_contacts(struct TariWallet *wallet,int* error_out);

// Get the TariCompletedTransactions from a TariWallet
struct TariCompletedTransactions *wallet_get_completed_transactions(struct TariWallet *wallet,int* error_out);

// Get the TariPendingOutboundTransactions from a TariWallet
struct TariPendingOutboundTransactions *wallet_get_pending_outbound_transactions(struct TariWallet *wallet,int* error_out);

// Get the TariPublicKey from a TariCommsConfig
struct TariPublicKey *wallet_get_public_key(struct TariWallet *wallet,int* error_out);

// Get the TariPendingInboundTransactions from a TariWallet
struct TariPendingInboundTransactions *wallet_get_pending_inbound_transactions(struct TariWallet *wallet,int* error_out);

// Get the TariCompletedTransaction from a TariWallet by its TransactionId
struct TariCompletedTransaction *wallet_get_completed_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id,int* error_out);

// Get the TariPendingOutboundTransaction from a TariWallet by its TransactionId
struct TariPendingOutboundTransaction *wallet_get_pending_outbound_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id,int* error_out);

// Get the TariPendingInboundTransaction from a TariWallet by its TransactionId
struct TariPendingInboundTransaction *wallet_get_pending_inbound_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id,int* error_out);

// Simulates completion of a TariPendingOutboundTransaction
bool wallet_test_complete_sent_transaction(struct TariWallet *wallet, struct TariPendingOutboundTransaction *tx,int* error_out);

// Checks if a TariCompletedTransaction was originally a TariPendingOutboundTransaction,
// i.e the transaction was originally sent from the wallet
bool wallet_is_completed_transaction_outbound(struct TariWallet *wallet, struct TariCompletedTransaction *tx,int* error_out);

// Import a UTXO into the wallet. This will add a spendable UTXO and create a faux completed transaction to record the
// event.
unsigned long long wallet_import_utxo(struct TariWallet *wallet, unsigned long long amount, struct TariPrivateKey *spending_key, struct TariPublicKey *source_public_key, const char *message, int* error_out);

// This function will tell the wallet to query the set base node to confirm the status of wallet data.
unsigned long long wallet_sync_with_base_node(struct TariWallet *wallet, int* error_out);

// Simulates the completion of a broadcasted TariPendingInboundTransaction
bool wallet_test_broadcast_transaction(struct TariWallet *wallet, unsigned long long tx, int* error_out);

// Simulates receiving the finalized version of a TariPendingInboundTransaction
bool wallet_test_finalize_received_transaction(struct TariWallet *wallet, struct TariPendingInboundTransaction *tx, int* error_out);

// Simulates a TariCompletedTransaction that has been mined
bool wallet_test_mine_transaction(struct TariWallet *wallet, unsigned long long tx, int* error_out);

// Simulates a TariPendingInboundtransaction being received
bool wallet_test_receive_transaction(struct TariWallet *wallet,int* error_out);

/// Cancel a Pending Outbound Transaction
bool wallet_cancel_pending_transaction(struct TariWallet *wallet, unsigned long long transaction_id, int* error_out);

// Frees memory for a TariWallet
void wallet_destroy(struct TariWallet *wallet);

/// This function will log the provided string at debug level. To be used to have a client log messages to the LibWallet
void log_debug_message(const char* msg);

#ifdef __cplusplus
}
#endif

#endif /* wallet_ffi_h */
