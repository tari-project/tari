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

#ifndef wallet_ffi_h
#define wallet_ffi_h

#ifdef __cplusplus
extern "C" {
#endif

#include <stdbool.h>
#include <stdio.h>

struct ByteVector;

struct TariCommsConfig;

struct TariPrivateKey;

struct TariWallet;

struct TariPublicKey;

struct TariPublicKeys;

struct TariCommitmentSignature;

struct TariCovenant;

struct TariOutputFeatures;

struct TariContacts;

struct TariContact;

struct TariCompletedTransactions;

struct TariBalance;

struct TariCompletedTransaction;

struct TariPendingOutboundTransactions;

struct TariPendingOutboundTransaction;

struct TariPendingInboundTransactions;

struct TariPendingInboundTransaction;

struct TariTransportType;

struct TariSeedWords;

struct EmojiSet;

struct TariTransactionKernel;

/// -------------------------------- Transport Types ----------------------------------------------- ///

// Creates a memory transport type
struct TariTransportType *transport_memory_create();

// Creates a tcp transport type
struct TariTransportType *transport_tcp_create(const char *listener_address, int *error_out);

// Creates a tor transport type
struct TariTransportType *transport_tor_create(
    const char *control_server_address,
    struct ByteVector *tor_cookie,
    unsigned short tor_port,
    bool tor_proxy_bypass_for_outbound,
    const char *socks_username,
    const char *socks_password,
    int *error_out);

// Gets the address from a memory transport type
char *transport_memory_get_address(struct TariTransportType *transport, int *error_out);

// Frees memory for a transport type
void transport_type_destroy(struct TariTransportType *transport);

/// -------------------------------- Strings ----------------------------------------------- ///

// Frees memory for a string pointer
void string_destroy(char *s);

/// -------------------------------- ByteVector ----------------------------------------------- ///

// Creates a ByteVector
struct ByteVector *byte_vector_create(const unsigned char *byte_array, unsigned int element_count, int *error_out);

// Gets a char from a ByteVector
unsigned char byte_vector_get_at(struct ByteVector *ptr, unsigned int i, int *error_out);

// Returns the number of elements in a ByteVector
unsigned int byte_vector_get_length(const struct ByteVector *vec, int *error_out);

// Frees memory for a ByteVector pointer
void byte_vector_destroy(struct ByteVector *bytes);

/// -------------------------------- TariPublicKey ----------------------------------------------- ///

// Creates a TariPublicKey from a ByteVector
struct TariPublicKey *public_key_create(struct ByteVector *bytes, int *error_out);

// Gets a ByteVector from a TariPublicKey
struct ByteVector *public_key_get_bytes(struct TariPublicKey *public_key, int *error_out);

// Creates a TariPublicKey from a TariPrivateKey
struct TariPublicKey *public_key_from_private_key(struct TariPrivateKey *secret_key, int *error_out);

// Creates a TariPublicKey from a const char* filled with hexadecimal characters
struct TariPublicKey *public_key_from_hex(const char *hex, int *error_out);

// Frees memory for a TariPublicKey pointer
void public_key_destroy(struct TariPublicKey *pk);

// Frees memory for a TariPublicKeys pointer
void public_keys_destroy(struct TariPublicKeys *pk);

//Converts a TariPublicKey to char array in emoji format
char *public_key_to_emoji_id(struct TariPublicKey *pk, int *error_out);

// Converts a char array in emoji format to a public key
struct TariPublicKey *emoji_id_to_public_key(const char *emoji, int *error_out);


/// -------------------------------- TariPrivateKey ----------------------------------------------- ///

// Creates a TariPrivateKey from a ByteVector
struct TariPrivateKey *private_key_create(struct ByteVector *bytes, int *error_out);

// Generates a TariPrivateKey
struct TariPrivateKey *private_key_generate();

// Creates a ByteVector from a TariPrivateKey
struct ByteVector *private_key_get_bytes(struct TariPrivateKey *private_key, int *error_out);

// Creates a TariPrivateKey from a const char* filled with hexadecimal characters
struct TariPrivateKey *private_key_from_hex(const char *hex, int *error_out);

// Frees memory for a TariPrivateKey
void private_key_destroy(struct TariPrivateKey *pk);

/// -------------------------------- Commitment Signature  --------------------------------------------- ///

// Creates a TariCommitmentSignature from `u`, `v` and `public_nonce` ByteVectors
struct TariCommitmentSignature *commitment_signature_create_from_bytes(
    struct ByteVector *public_nonce_bytes,
    struct ByteVector *u_bytes,
    struct ByteVector *v_bytes,
    int *error_out
);

// Frees memory for a TariCommitmentSignature
void commitment_signature_destroy(struct TariCommitmentSignature *com_sig);

/// -------------------------------- Seed Words  -------------------------------------------------- ///
// Create an empty instance of TariSeedWords
struct TariSeedWords *seed_words_create();

// Create a TariSeedWords instance containing the entire mnemonic wordlist for the requested language
struct TariSeedWords *seed_words_get_mnemonic_word_list_for_language(const char *language, int *error_out);

// Get the number of seed words in the provided collection
unsigned int seed_words_get_length(struct TariSeedWords *seed_words, int *error_out);

// Get a seed word from the provided collection at the specified position
char *seed_words_get_at(struct TariSeedWords *seed_words, unsigned int position, int *error_out);

/// Add a word to the provided TariSeedWords instance
///
/// ## Arguments
/// `seed_words` - The pointer to a TariSeedWords
/// `word` - Word to add
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// 'c_uchar' - Returns a u8 version of the `SeedWordPushResult` enum indicating whether the word was not a valid seed
/// word, if the push was successful and whether the push was successful and completed the full Seed Phrase
///     '0' -> InvalidSeedWord
///     '1' -> SuccessfulPush
///     '2' -> SeedPhraseComplete
///     '3' -> InvalidSeedPhrase
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
unsigned char seed_words_push_word(struct TariSeedWords *seed_words, const char *word, int *error_out);

// Frees the memory for a TariSeedWords collection
void seed_words_destroy(struct TariSeedWords *seed_words);

/// -------------------------------- Contact ------------------------------------------------------ ///

// Creates a TariContact
struct TariContact *contact_create(const char *alias, struct TariPublicKey *public_key, int *error_out);

// Gets the alias of the TariContact
char *contact_get_alias(struct TariContact *contact, int *error_out);

/// Gets the TariPublicKey of the TariContact
struct TariPublicKey *contact_get_public_key(struct TariContact *contact, int *error_out);

// Frees memory for a TariContact
void contact_destroy(struct TariContact *contact);

/// -------------------------------- Contacts ------------------------------------------------------ ///

// Gets the number of elements of TariContacts
unsigned int contacts_get_length(struct TariContacts *contacts, int *error_out);

// Gets a TariContact from TariContacts at position
struct TariContact *contacts_get_at(struct TariContacts *contacts, unsigned int position, int *error_out);

// Frees memory for TariContacts
void contacts_destroy(struct TariContacts *contacts);

/// -------------------------------- CompletedTransaction ------------------------------------------------------ ///

// Gets the destination TariPublicKey of a TariCompletedTransaction
struct TariPublicKey *completed_transaction_get_destination_public_key(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the source TariPublicKey of a TariCompletedTransaction
struct TariPublicKey *completed_transaction_get_source_public_key(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the amount of a TariCompletedTransaction
unsigned long long completed_transaction_get_amount(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the fee of a TariCompletedTransaction
unsigned long long completed_transaction_get_fee(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the message of a TariCompletedTransaction
const char *completed_transaction_get_message(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the status of a TariCompletedTransaction
// | Value | Interpretation   |
// |---|---|
// |  -1 | TxNullError        |
// |   0 | Completed          |
// |   1 | Broadcast          |
// |   2 | MinedUnconfirmed   |
// |   3 | Imported           |
// |   4 | Pending            |
// |   5 | Coinbase           |
// |   6 | MinedConfirmed     |
// |   7 | Rejected           |
// |   8 | FauxUnconfirmed    |
// |   9 | FauxConfirmed      |
int completed_transaction_get_status(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the TransactionID of a TariCompletedTransaction
unsigned long long completed_transaction_get_transaction_id(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the timestamp of a TariCompletedTransaction
unsigned long long completed_transaction_get_timestamp(struct TariCompletedTransaction *transaction, int *error_out);

// Checks if a TariCompletedTransaction was originally a TariPendingOutboundTransaction,
// i.e the transaction was originally sent from the wallet
bool completed_transaction_is_outbound(struct TariCompletedTransaction *tx, int *error_out);

/// Gets the number of confirmations of a TariCompletedTransaction
unsigned long long completed_transaction_get_confirmations(struct TariCompletedTransaction *transaction, int *error_out);

// Gets the TariTransactionKernel of a TariCompletedTransaction
struct TariTransactionKernel *completed_transaction_get_transaction_kernel(struct TariCompletedTransaction *transaction, int *error_out);

/// Gets the reason a TariCompletedTransaction is cancelled, if it is indeed cancelled
///
/// ## Arguments
/// `tx` - The TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_int` - Returns the reason for cancellation which corresponds to:
/// | Value | Interpretation    |
/// |---  |---                  |
/// |  -1 | Not Cancelled       |
/// |   0 | Unknown             |
/// |   1 | UserCancelled       |
/// |   2 | Timeout             |
/// |   3 | DoubleSpend         |
/// |   4 | Orphan              |
/// |   5 | TimeLocked          |
/// |   6 | InvalidTransaction  |
/// |   7 | AbandonedCoinbase   |
/// # Safety
/// None
int completed_transaction_get_cancellation_reason(struct TariCompletedTransaction *transaction, int *error_out);

// Frees memory for a TariCompletedTransaction
void completed_transaction_destroy(struct TariCompletedTransaction *transaction);

/// --------------------------------------- TransactionKernel ------------------------------------------------------ ///

// Gets the excess for a TariTransactionKernel
const char *transaction_kernel_get_excess_hex(struct TariTransactionKernel *kernel, int *error_out);

// Gets the public nonce for a TariTransactionKernel
const char *transaction_kernel_get_excess_public_nonce_hex(struct TariTransactionKernel *kernel, int *error_out);

// Gets the signature for a TariTransactionKernel
const char *transaction_kernel_get_excess_signature_hex(struct TariTransactionKernel *kernel, int *error_out);

// Frees memory for a TariTransactionKernel
void transaction_kernel_destroy(struct TariTransactionKernel *kernel);

/// -------------------------------- CompletedTransactions ------------------------------------------------------ ///

// Gets number of elements in TariCompletedTransactions
unsigned int completed_transactions_get_length(struct TariCompletedTransactions *transactions, int *error_out);

// Gets a TariCompletedTransaction from a TariCompletedTransactions at position
struct TariCompletedTransaction *completed_transactions_get_at(struct TariCompletedTransactions *transactions, unsigned int position, int *error_out);

// Frees memory for a TariCompletedTransactions
void completed_transactions_destroy(struct TariCompletedTransactions *transactions);

/// -------------------------------- OutboundTransaction ------------------------------------------------------ ///

// Gets the TransactionId of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_transaction_id(struct TariPendingOutboundTransaction *transaction, int *error_out);

// Gets the destination TariPublicKey of a TariPendingOutboundTransaction
struct TariPublicKey *pending_outbound_transaction_get_destination_public_key(struct TariPendingOutboundTransaction *transaction, int *error_out);

// Gets the amount of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_amount(struct TariPendingOutboundTransaction *transaction, int *error_out);

// Gets the fee of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_fee(struct TariPendingOutboundTransaction *transaction, int *error_out);

// Gets the message of a TariPendingOutboundTransaction
const char *pending_outbound_transaction_get_message(struct TariPendingOutboundTransaction *transaction, int *error_out);

// Gets the timestamp of a TariPendingOutboundTransaction
unsigned long long pending_outbound_transaction_get_timestamp(struct TariPendingOutboundTransaction *transaction, int *error_out);

// Gets the status of a TariPendingOutboundTransaction
// | Value | Interpretation   |
// |---|---|
// |  -1 | TxNullError        |
// |   0 | Completed          |
// |   1 | Broadcast          |
// |   2 | MinedUnconfirmed   |
// |   3 | Imported           |
// |   4 | Pending            |
// |   5 | Coinbase           |
// |   6 | MinedConfirmed     |
// |   7 | Rejected           |
// |   8 | FauxUnconfirmed    |
// |   9 | FauxConfirmed      |
int pending_outbound_transaction_get_status(struct TariPendingOutboundTransaction *transaction, int *error_out);

// Frees memory for a TariPendingOutboundTactions
void pending_outbound_transaction_destroy(struct TariPendingOutboundTransaction *transaction);

/// -------------------------------- OutboundTransactions ------------------------------------------------------ ///

// Gets the number of elements in a TariPendingOutboundTactions
unsigned int pending_outbound_transactions_get_length(struct TariPendingOutboundTransactions *transactions, int *error_out);

// Gets a TariPendingOutboundTransaction of a TariPendingOutboundTransactions at position
struct TariPendingOutboundTransaction *pending_outbound_transactions_get_at(struct TariPendingOutboundTransactions *transactions, unsigned int position, int *error_out);

// Frees memory of a TariPendingOutboundTransactions
void pending_outbound_transactions_destroy(struct TariPendingOutboundTransactions *transactions);

/// -------------------------------- InboundTransaction ------------------------------------------------------ ///

// Gets the TransactionId of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_transaction_id(struct TariPendingInboundTransaction *transaction, int *error_out);

// Gets the source TariPublicKey of a TariPendingInboundTransaction
struct TariPublicKey *pending_inbound_transaction_get_source_public_key(struct TariPendingInboundTransaction *transaction, int *error_out);

// Gets the message of a TariPendingInboundTransaction
const char *pending_inbound_transaction_get_message(struct TariPendingInboundTransaction *transaction, int *error_out);

// Gets the amount of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_amount(struct TariPendingInboundTransaction *transaction, int *error_out);

// Gets the timestamp of a TariPendingInboundTransaction
unsigned long long pending_inbound_transaction_get_timestamp(struct TariPendingInboundTransaction *transaction, int *error_out);

// Gets the status of a TariPendingInboundTransaction
// | Value | Interpretation   |
// |---|---|
// |  -1 | TxNullError        |
// |   0 | Completed          |
// |   1 | Broadcast          |
// |   2 | MinedUnconfirmed   |
// |   3 | Imported           |
// |   4 | Pending            |
// |   5 | Coinbase           |
// |   6 | MinedConfirmed     |
// |   7 | Rejected           |
// |   8 | FauxUnconfirmed |
// |   9 | FauxConfirmed   |
int pending_inbound_transaction_get_status(struct TariPendingInboundTransaction *transaction, int *error_out);

// Frees memory for a TariPendingInboundTransaction
void pending_inbound_transaction_destroy(struct TariPendingInboundTransaction *transaction);

/// -------------------------------- InboundTransactions ------------------------------------------------------ ///

// Gets the number of elements in a TariPendingInboundTransactions
unsigned int pending_inbound_transactions_get_length(struct TariPendingInboundTransactions *transactions, int *error_out);

// Gets a TariPendingInboundTransaction of a TariPendingInboundTransactions at position
struct TariPendingInboundTransaction *pending_inbound_transactions_get_at(struct TariPendingInboundTransactions *transactions, unsigned int position, int *error_out);

// Frees memory of a TariPendingInboundTransaction
void pending_inbound_transactions_destroy(struct TariPendingInboundTransactions *transactions);

/// -------------------------------- TariCommsConfig ----------------------------------------------- ///
// Creates a TariCommsConfig
// Valid values for network are: dibbler, igor, localnet, mainnet
struct TariCommsConfig *comms_config_create(const char *public_address,
                                            struct TariTransportType *transport,
                                            const char *database_name,
                                            const char *datastore_path,
                                            unsigned long long discovery_timeout_in_secs,
                                            unsigned long long saf_message_duration_in_secs,
                                            const char *network,
                                            int *error_out);

// Frees memory for a TariCommsConfig
void comms_config_destroy(struct TariCommsConfig *wc);

// Converts a char array in emoji format to a public key
struct TariPublicKeys *comms_list_connected_public_keys(struct TariWallet *wallet, int *error_out);
/// -------------------------------- TariWallet ----------------------------------------------- //

/// Creates a TariWallet
///
/// ## Arguments
/// `config` - The TariCommsConfig pointer
/// `log_path` - An optional file path to the file where the logs will be written. If no log is required pass *null*
/// pointer.
/// `num_rolling_log_files` - Specifies how many rolling log files to produce, if no rolling files are wanted then set
/// this to 0
/// `size_per_log_file_bytes` - Specifies the size, in bytes, at which the logs files will roll over, if no
/// rolling files are wanted then set this to 0
/// `passphrase` - An optional string that represents the passphrase used to
/// encrypt/decrypt the databases for this wallet. If it is left Null no encryption is used. If the databases have been
/// encrypted then the correct passphrase is required or this function will fail.
/// `callback_received_transaction` - The callback function pointer matching the
/// function signature. This will be called when an inbound transaction is received.
/// `callback_received_transaction_reply` - The callback function pointer matching the function signature. This will be
/// called when a reply is received for a pending outbound transaction
/// `callback_received_finalized_transaction` - The callback function pointer matching the function signature. This will
/// be called when a Finalized version on an Inbound transaction is received
/// `callback_transaction_broadcast` - The callback function pointer matching the function signature. This will be
/// called when a Finalized transaction is detected a Broadcast to a base node mempool.
/// `callback_transaction_mined` - The callback function pointer matching the function signature. This will be called
/// when a Broadcast transaction is detected as mined AND confirmed.
/// `callback_transaction_mined_unconfirmed` - The callback function pointer matching the function signature. This will
/// be called  when a Broadcast transaction is detected as mined but not yet confirmed.
/// `callback_faux_transaction_confirmed` - The callback function pointer matching the function signature. This will be called
/// when a one-sided transaction is detected as mined AND confirmed.
/// `callback_faux_transaction_unconfirmed` - The callback function pointer matching the function signature. This will
/// be called  when a one-sided transaction is detected as mined but not yet confirmed.
/// `callback_direct_send_result` - The callback function pointer matching the function signature. This is called
/// when a direct send is completed. The first parameter is the transaction id and the second is whether if was successful or not.
/// `callback_store_and_forward_send_result` - The callback function pointer matching the function signature. This is called
/// when a direct send is completed. The first parameter is the transaction id and the second is whether if was successful or not.
/// `callback_transaction_cancellation` - The callback function pointer matching the function signature. This is called
/// when a transaction is cancelled. The first parameter is a pointer to the cancelled transaction, the second is a reason as to
/// why said transaction failed that is mapped to the `TxCancellationReason` enum:
/// pub enum TxCancellationReason {
///     Unknown,                // 0
///     UserCancelled,          // 1
///     Timeout,                // 2
///     DoubleSpend,            // 3
///     Orphan,                 // 4
///     TimeLocked,             // 5
///     InvalidTransaction,     // 6
/// }
/// `callback_txo_validation_complete` - The callback function pointer matching the function signature. This is called
/// when a TXO validation process is completed. The request_key is used to identify which request this
/// callback references and the second parameter is a is a bool that returns if the validation was successful or not.
/// `callback_balance_updated` - The callback function pointer matching the function signature. This is called whenever
/// the balance changes.
/// `callback_transaction_validation_complete` - The callback function pointer matching the function signature. This is
/// called when a Transaction validation process is completed. The request_key is used to identify which request this
/// callback references and the second parameter is a is a bool that returns if the validation was successful or not.
/// `callback_saf_message_received` - The callback function pointer that will be called when the Dht has determined that
/// is has connected to enough of its neighbours to be confident that it has received any SAF messages that were waiting
/// for it.
/// `callback_connectivity_status` -  This callback is called when the status of connection to the set base node changes.
/// it will return an enum encoded as an integer as follows:
/// pub enum OnlineStatus {
///     Connecting,     // 0
///     Online,         // 1
///     Offline,        // 2
/// }
/// `recovery_in_progress` - Pointer to an bool which will be modified to indicate if there is an outstanding recovery
/// that should be completed or not to an error code should one occur, may not be null. Functions as an out parameter.
/// `error_out` - Pointer to an int which will be modified
/// to an error code should one occur, may not be null. Functions as an out parameter.
/// ## Returns
/// `*mut TariWallet` - Returns a pointer to a TariWallet, note that it returns ptr::null_mut()
/// if config is null, a wallet error was encountered or if the runtime could not be created
///
/// # Safety
/// The ```wallet_destroy``` method must be called when finished with a TariWallet to prevent a memory leak
///
/// The CallbackValidationResults enum can return the following values:
/// enum CallbackValidationResults {
///        Success,           // 0
///        Aborted,           // 1
///        Failure,           // 2
///        BaseNodeNotInSync, // 3
///    }
struct TariWallet *wallet_create(struct TariCommsConfig *config,
                                 const char *log_path,
                                 unsigned int num_rolling_log_files,
                                 unsigned int size_per_log_file_bytes,
                                 const char *passphrase,
                                 struct TariSeedWords *seed_words,
                                 void (*callback_received_transaction)(struct TariPendingInboundTransaction *),
                                 void (*callback_received_transaction_reply)(struct TariCompletedTransaction *),
                                 void (*callback_received_finalized_transaction)(struct TariCompletedTransaction *),
                                 void (*callback_transaction_broadcast)(struct TariCompletedTransaction *),
                                 void (*callback_transaction_mined)(struct TariCompletedTransaction *),
                                 void (*callback_transaction_mined_unconfirmed)(struct TariCompletedTransaction *, unsigned long long),
                                 void (*callback_faux_transaction_confirmed)(struct TariCompletedTransaction *),
                                 void (*callback_faux_transaction_unconfirmed)(struct TariCompletedTransaction *, unsigned long long),
                                 void (*callback_direct_send_result)(unsigned long long, bool),
                                 void (*callback_store_and_forward_send_result)(unsigned long long, bool),
                                 void (*callback_transaction_cancellation)(struct TariCompletedTransaction *, unsigned long long),
                                 void (*callback_txo_validation_complete)(unsigned long long, bool),
                                 void (*callback_balance_updated)(struct TariBalance *),
                                 void (*callback_transaction_validation_complete)(unsigned long long, bool),
                                 void (*callback_saf_message_received)(),
                                 void (*callback_connectivity_status)(unsigned long long),
                                 bool *recovery_in_progress,
                                 int *error_out);

// Gets the balance
struct TariBalance *wallet_get_balance(struct TariWallet *wallet, int *error_out);

// Signs a message
char *wallet_sign_message(struct TariWallet *wallet, const char *msg, int *error_out);

// Verifies signature for a signed message
bool wallet_verify_message_signature(struct TariWallet *wallet, struct TariPublicKey *public_key, const char *hex_sig_nonce, const char *msg, int *error_out);

// Adds a base node peer to the TariWallet
bool wallet_add_base_node_peer(struct TariWallet *wallet, struct TariPublicKey *public_key, const char *address, int *error_out);

// Upserts a TariContact to the TariWallet, if the contact does not exist it is inserted and if it does the alias is updated
bool wallet_upsert_contact(struct TariWallet *wallet, struct TariContact *contact, int *error_out);

// Removes a TariContact form the TariWallet
bool wallet_remove_contact(struct TariWallet *wallet, struct TariContact *contact, int *error_out);

// Gets the available balance from a TariBalance
unsigned long long balance_get_available(struct TariBalance *balance, int *error_out);

// Gets the available balance from a TariBalance
unsigned long long balance_get_time_locked(struct TariBalance *balance, int *error_out);

// Gets the available balance from a TariBalance
unsigned long long balance_get_pending_incoming(struct TariBalance *balance, int *error_out);

// Gets the available balance from a TariBalance
unsigned long long balance_get_pending_outgoing(struct TariBalance *balance, int *error_out);

// Get a fee estimate from a TariWallet for a given amount
unsigned long long wallet_get_fee_estimate(struct TariWallet *wallet, unsigned long long amount, unsigned long long fee_per_gram, unsigned long long num_kernels, unsigned long long num_outputs, int *error_out);

// Get the number of mining confirmations by the wallet transaction service
unsigned long long wallet_get_num_confirmations_required(struct TariWallet *wallet, int *error_out);

// Set the number of mining confirmations by the wallet transaction service
void wallet_set_num_confirmations_required(struct TariWallet *wallet, unsigned long long num, int *error_out);

// Sends a TariPendingOutboundTransaction
unsigned long long wallet_send_transaction(struct TariWallet *wallet, struct TariPublicKey *destination, unsigned long long amount, unsigned long long fee_per_gram, const char *message, bool one_sided, int *error_out);

// Get the TariContacts from a TariWallet
struct TariContacts *wallet_get_contacts(struct TariWallet *wallet, int *error_out);

// Get the TariCompletedTransactions from a TariWallet
struct TariCompletedTransactions *wallet_get_completed_transactions(struct TariWallet *wallet, int *error_out);

// Get the TariPendingOutboundTransactions from a TariWallet
struct TariPendingOutboundTransactions *wallet_get_pending_outbound_transactions(struct TariWallet *wallet, int *error_out);

// Get the TariPublicKey from a TariCommsConfig
struct TariPublicKey *wallet_get_public_key(struct TariWallet *wallet, int *error_out);

// Get the TariPendingInboundTransactions from a TariWallet
struct TariPendingInboundTransactions *wallet_get_pending_inbound_transactions(struct TariWallet *wallet, int *error_out);

// Get all cancelled transactions from a TariWallet
struct TariCompletedTransactions *wallet_get_cancelled_transactions(struct TariWallet *wallet, int *error_out);

// Get the TariCompletedTransaction from a TariWallet by its TransactionId
struct TariCompletedTransaction *wallet_get_completed_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id, int *error_out);

// Get the TariPendingOutboundTransaction from a TariWallet by its TransactionId
struct TariPendingOutboundTransaction *wallet_get_pending_outbound_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id, int *error_out);

// Get the TariPendingInboundTransaction from a TariWallet by its TransactionId
struct TariPendingInboundTransaction *wallet_get_pending_inbound_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id, int *error_out);

// Get a Cancelled transaction from a TariWallet by its TransactionId. Pending Inbound or Outbound transaction will be converted to a CompletedTransaction
struct TariCompletedTransaction *wallet_get_cancelled_transaction_by_id(struct TariWallet *wallet, unsigned long long transaction_id, int *error_out);

// Import a UTXO into the wallet. This will add a spendable UTXO and create a faux completed transaction to record the
// event.
unsigned long long wallet_import_utxo(
    struct TariWallet *wallet,
    unsigned long long amount,
    struct TariPrivateKey *spending_key,
    struct TariPublicKey *source_public_key,
    struct TariOutputFeatures *features,
    struct TariCommitmentSignature *metadata_signature,
    struct TariPublicKey *sender_offset_public_key,
    struct TariPrivateKey *script_private_key,
    struct TariCovenant *covenant,
    const char *message,
    int *error_out
);

// This function will tell the wallet to query the set base node to confirm the status of unspent transaction outputs (UTXOs).
unsigned long long wallet_start_txo_validation(struct TariWallet *wallet, int *error_out);

//This function will tell the wallet to query the set base node to confirm the status of mined transactions.
unsigned long long wallet_start_transaction_validation(struct TariWallet *wallet, int *error_out);

//This function will tell the wallet retart any broadcast protocols for completed transactions. Ideally this should be
// called after a successfuly Transaction Validation is complete
bool wallet_restart_transaction_broadcast(struct TariWallet *wallet, int *error_out);

// Set the power mode of the wallet to Low Power mode which will reduce the amount of network operations the wallet performs to conserve power
void wallet_set_low_power_mode(struct TariWallet *wallet, int *error_out);

// Set the power mode of the wallet to Normal Power mode which will then use the standard level of network traffic
void wallet_set_normal_power_mode(struct TariWallet *wallet, int *error_out);

/// Cancel a Pending Outbound Transaction
bool wallet_cancel_pending_transaction(struct TariWallet *wallet, unsigned long long transaction_id, int *error_out);

/// Perform a coin split
unsigned long long wallet_coin_split(struct TariWallet *wallet, unsigned long long amount, unsigned long long count, unsigned long long fee, const char *msg, unsigned long long lock_height, int *error_out);

/// Get the seed words representing the seed private key of the provided TariWallet
struct TariSeedWords *wallet_get_seed_words(struct TariWallet *wallet, int *error_out);

// Apply encryption to the databases used in this wallet using the provided passphrase. If the databases are already
// encrypted this function will fail.
void wallet_apply_encryption(struct TariWallet *wallet, const char *passphrase, int *error_out);

// Remove encryption to the databases used in this wallet. If this wallet is currently encrypted this encryption will
// be removed. If it is not encrypted then this function will still succeed to make the operation idempotent
void wallet_remove_encryption(struct TariWallet *wallet, int *error_out);

/// Set a Key Value in the Wallet storage used for Client Key Value store
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `key` - The pointer to a Utf8 string representing the Key
/// `value` - The pointer to a Utf8 string representing the Value ot be stored
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating the operation's success or failure. The error_ptr will hold the error
/// code if there was a failure
///
/// # Safety
/// None
bool wallet_set_key_value(struct TariWallet *wallet, const char *key, const char *value, int *error_out);

/// get a stored Value that was previously stored in the Wallet storage used for Client Key Value store
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `key` - The pointer to a Utf8 string representing the Key
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array of the Value string. Note that it returns an null pointer if an
/// error occured.
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
const char *wallet_get_value(struct TariWallet *wallet, const char *key, int *error_out);

/// Clears a Value for the provided Key Value in the Wallet storage used for Client Key Value store
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `key` - The pointer to a Utf8 string representing the Key
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating the operation's success or failure. The error_ptr will hold the error
/// code if there was a failure
///
/// # Safety
/// None
bool wallet_clear_value(struct TariWallet *wallet, const char *key, int *error_out);

/// Check if a Wallet has the data of an In Progress Recovery in its database.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating whether there is an in progress recovery or not. An error will also
/// result in a false result.
///
/// # Safety
/// None
bool wallet_is_recovery_in_progress(struct TariWallet *wallet, int *error_out);

/// Starts the Wallet recovery process.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `base_node_public_key` - The TariPublicKey pointer of the Base Node the recovery process will use
/// `recovery_progress_callback` - The callback function pointer that will be used to asynchronously communicate
/// progress to the client. The first argument of the callback is an event enum encoded as a u8 as follows:
/// ```
/// enum RecoveryEvent {
///     ConnectingToBaseNode,       // 0
///     ConnectedToBaseNode,        // 1
///     ConnectionToBaseNodeFailed, // 2
///     Progress,                   // 3
///     Completed,                  // 4
///     ScanningRoundFailed,        // 5
///     RecoveryFailed,             // 6
/// }
/// ```
/// The second and third arguments are u64 values that will contain different information depending on the event
/// that triggered the callback. The meaning of the second and third argument for each event are as follows:
///     - ConnectingToBaseNode, 0, 0
///     - ConnectedToBaseNode, 0, 1
///     - ConnectionToBaseNodeFailed, number of retries, retry limit
///     - Progress, current block, total number of blocks
///     - Completed, total number of UTXO's recovered, MicroTari recovered,
///     - ScanningRoundFailed, number of retries, retry limit
///     - RecoveryFailed, 0, 0
///
/// If connection to a base node is successful the flow of callbacks should be:
///     - The process will start with a callback with `ConnectingToBaseNode` showing a connection is being attempted
///       this could be repeated multiple times until a connection is made.
///     - The next a callback with `ConnectedToBaseNode` indicate a successful base node connection and process has
///       started
///     - In Progress callbacks will be of the form (n, m) where n < m
///     - If the process completed successfully then the final `Completed` callback will return how many UTXO's were
///       scanned and how much MicroTari was recovered
///     - If there is an error in the connection process then the `ConnectionToBaseNodeFailed` will be returned
///     - If there is a minor error in scanning then `ScanningRoundFailed` will be returned and another connection/sync
///       attempt will be made
///     - If a unrecoverable error occurs the `RecoveryFailed` event will be returned and the client will need to start
///       a new process.
///
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating whether the process started successfully or not, the process will
/// continue to run asynchronously and communicate it progress via the callback. An error will also produce a false
/// result.
///
/// # Safety
/// None
bool wallet_start_recovery(struct TariWallet *wallet, struct TariPublicKey *base_node_public_key, void (*recovery_progress_callback)(unsigned char, unsigned long long, unsigned long long), int *error_out);

// Frees memory for a TariWallet
void wallet_destroy(struct TariWallet *wallet);

// Frees memory for a TariBalance
void balance_destroy(struct TariBalance *balance);

// This function will produce a partial backup of the specified wallet database file (full file path must be provided.
// This backup will be written to the provided file (full path must include the filename and extension) and will include
// the full wallet db but will clear the sensitive Comms Private Key
void file_partial_backup(const char *original_file_path, const char *backup_file_path, int *error_out);

/// This function will log the provided string at debug level. To be used to have a client log messages to the LibWallet
void log_debug_message(const char *msg, int *error_out);

struct EmojiSet *get_emoji_set();

void emoji_set_destroy(struct EmojiSet *emoji_set);

struct ByteVector *emoji_set_get_at(struct EmojiSet *emoji_set, unsigned int position, int *error_out);

unsigned int emoji_set_get_length(struct EmojiSet *emoji_set, int *error_out);

#ifdef __cplusplus
}
#endif

#endif /* wallet_ffi_h */
