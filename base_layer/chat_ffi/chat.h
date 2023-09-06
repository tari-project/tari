// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

// This file was generated by cargo-bindgen. Please do not edit manually.

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

struct ApplicationConfig;

struct ByteVector;

struct ChatMessages;

struct ClientFFI;

struct TariAddress;

struct TransportConfig;

struct ChatFFIContactsLivenessData {
  const char *address;
  uint64_t last_seen;
  uint8_t online_status;
};

typedef void (*CallbackContactStatusChange)(struct ChatFFIContactsLivenessData*);

struct ChatFFIMessage {
  const char *body;
  const char *from_address;
  uint64_t stored_at;
  const char *message_id;
};

typedef void (*CallbackMessageReceived)(struct ChatFFIMessage*);

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Creates a Chat Client
 *
 * ## Arguments
 * `config` - The ApplicationConfig pointer
 * `identity_file_path` - The path to the node identity file
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `*mut ChatClient` - Returns a pointer to a ChatClient, note that it returns ptr::null_mut()
 * if any error was encountered or if the runtime could not be created.
 *
 * # Safety
 * The ```destroy_client``` method must be called when finished with a ClientFFI to prevent a memory leak
 */
struct ClientFFI *create_chat_client(struct ApplicationConfig *config,
                                     int *error_out,
                                     CallbackContactStatusChange callback_contact_status_change,
                                     CallbackMessageReceived callback_message_received);

/**
 * Frees memory for a ClientFFI
 *
 * ## Arguments
 * `client` - The pointer of a ClientFFI
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_client_ffi(struct ClientFFI *client);

/**
 * Creates a Chat Client config
 *
 * ## Arguments
 * `network` - The network to run on
 * `public_address` - The nodes public address
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `*mut ApplicationConfig` - Returns a pointer to an ApplicationConfig
 *
 * # Safety
 * The ```destroy_config``` method must be called when finished with a Config to prevent a memory leak
 */
struct ApplicationConfig *create_chat_config(const char *network_str,
                                             const char *public_address,
                                             const char *datastore_path,
                                             const char *identity_file_path,
                                             struct TransportConfig *tor_transport_config,
                                             const char *log_path,
                                             int *error_out);

/**
 * Frees memory for an ApplicationConfig
 *
 * ## Arguments
 * `config` - The pointer of an ApplicationConfig
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_config(struct ApplicationConfig *config);

/**
 * Sends a message over a client
 *
 * ## Arguments
 * `client` - The Client pointer
 * `receiver` - A string containing a tari address
 * `message` - The peer seeds config for the node
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * The ```receiver``` should be destroyed after use
 */
void send_message(struct ClientFFI *client,
                  struct TariAddress *receiver,
                  const char *message_c_char,
                  int *error_out);

/**
 * Add a contact
 *
 * ## Arguments
 * `client` - The Client pointer
 * `address` - A TariAddress ptr
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * The ```address``` should be destroyed after use
 */
void add_contact(struct ClientFFI *client, struct TariAddress *receiver, int *error_out);

/**
 * Check the online status of a contact
 *
 * ## Arguments
 * `client` - The Client pointer
 * `address` - A TariAddress ptr
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * The ```address``` should be destroyed after use
 */
int check_online_status(struct ClientFFI *client, struct TariAddress *receiver, int *error_out);

/**
 * Get a ptr to all messages from or to address
 *
 * ## Arguments
 * `client` - The Client pointer
 * `address` - A TariAddress ptr
 * `limit` - The amount of messages you want to fetch. Default to 35, max 2500
 * `page` - The page of results you'd like returned. Default to 0, maximum of u64 max
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * The ```address``` should be destroyed after use
 * The returned pointer to ```*mut ChatMessages``` should be destroyed after use
 */
struct ChatMessages *get_messages(struct ClientFFI *client,
                                  struct TariAddress *address,
                                  int *limit,
                                  int *page,
                                  int *error_out);

/**
 * Frees memory for messages
 *
 * ## Arguments
 * `messages_ptr` - The pointer of a Vec<Message>
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_messages(struct ChatMessages *messages_ptr);

/**
 * Creates a TariAddress and returns a ptr
 *
 * ## Arguments
 * `receiver_c_char` - A string containing a tari address hex value
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `*mut TariAddress` - A ptr to a TariAddress
 *
 * # Safety
 * The ```destroy_tari_address``` function should be called when finished with the TariAddress
 */
struct TariAddress *create_tari_address(const char *receiver_c_char, int *error_out);

/**
 * Frees memory for a TariAddress
 *
 * ## Arguments
 * `address` - The pointer of a TariAddress
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_tari_address(struct TariAddress *address);

/**
 * Creates a tor transport type
 *
 * ## Arguments
 * `control_server_address` - The pointer to a char array
 * `tor_cookie` - The pointer to a ByteVector containing the contents of the tor cookie file, can be null
 * `tor_port` - The tor port
 * `tor_proxy_bypass_for_outbound` - Whether tor will use a direct tcp connection for a given bypass address instead of
 * the tor proxy if tcp is available, if not it has no effect
 * `socks_password` - The pointer to a char array containing the socks password, can be null
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TransportConfig` - Returns a pointer to a tor TransportConfig, null on error.
 *
 * # Safety
 * The ```transport_config_destroy``` method must be called when finished with a TransportConfig to prevent a
 * memory leak
 */
struct TransportConfig *transport_tor_create(const char *control_server_address,
                                             const struct ByteVector *tor_cookie,
                                             unsigned short tor_port,
                                             bool tor_proxy_bypass_for_outbound,
                                             const char *socks_username,
                                             const char *socks_password,
                                             int *error_out);

/**
 * Frees memory for a TransportConfig
 *
 * ## Arguments
 * `transport` - The pointer to a TransportConfig
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void transport_config_destroy(struct TransportConfig *transport);

/**
 * Frees memory for a TransportConfig
 *
 * ## Arguments
 * `transport` - The pointer to a TransportConfig
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_chat_ffi_message(struct ChatFFIMessage *address);

/**
 * Frees memory for a ChatFFIContactsLivenessData
 *
 * ## Arguments
 * `address` - The pointer of a ChatFFIContactsLivenessData
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_chat_ffi_liveness_data(struct ChatFFIContactsLivenessData *address);

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus
