// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

// This file was generated by cargo-bindgen. Please do not edit manually.

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

struct ChatMessages;

struct ClientFFI;

struct ContactsLivenessData;

struct Message;

struct TariAddress;

typedef void (*CallbackContactStatusChange)(struct ContactsLivenessData*);

typedef void (*CallbackMessageReceived)(struct Message*);

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
struct ClientFFI *create_chat_client(ApplicationConfig *config,
                                     const char *identity_file_path,
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
ApplicationConfig *create_chat_config(const char *network_str,
                                      const char *public_address,
                                      const char *datastore_path,
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
void destroy_config(ApplicationConfig *config);

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
 * `error_out` - Pointer to an int which will be modified
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * The ```address``` should be destroyed after use
 * The returned pointer to ```*mut ChatMessages``` should be destroyed after use
 */
struct ChatMessages *get_all_messages(struct ClientFFI *client,
                                      struct TariAddress *address,
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

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus
