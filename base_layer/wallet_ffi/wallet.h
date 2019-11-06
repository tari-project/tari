//  wallet_ffi.h

/*
    Package MobileWallet
    Created by David Main on 10/30/19
    Using Swift 5.0
    Running on macOS 10.14

    Copyright 2019 The Tari Project

    Redistribution and use in source and binary forms, with or
    without modification, are permitted provided that the
    following conditions are met:

    1. Redistributions of source code must retain the above copyright notice,
    this list of conditions and the following disclaimer.

    2. Redistributions in binary form must reproduce the above
    copyright notice, this list of conditions and the following disclaimer in the
    documentation and/or other materials provided with the distribution.

    3. Neither the name of the copyright holder nor the names of
    its contributors may be used to endorse or promote products
    derived from this software without specific prior written permission.

    THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND
    CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
    INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
    OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
    DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
    CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
    SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
    NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
    LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
    HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
    CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
    OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
    SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
*/

#ifndef wallet_ffi_h
#define wallet_ffi_h

#include <stdio.h>
#include <stdbool.h>

struct ByteVector;

struct TariKeyManagerSeedWords;

struct TariCommsConfig;

struct TariPrivateKey;

struct TariWallet;

struct TariWalletConfig;

struct TariPublicKey;

/// -------------------------------- Strings ----------------------------------------------- ///

void free_string(char *s);

/// -------------------------------- ByteVector ----------------------------------------------- ///

struct ByteVector *byte_vector_create(const unsigned char *byte_array, int element_count);

unsigned char byte_vector_get_at(struct ByteVector *ptr, int i);

int byte_vector_get_length(const struct ByteVector *vec);

struct ByteVector *public_key_get_key(struct TariPublicKey *pk);

void byte_vector_destroy(struct ByteVector *bytes);

/// -------------------------------- TariCommsConfig ----------------------------------------------- ///

struct TariCommsConfig *comms_config_create(char *address,
                                     char *database_name,
                                     char *datastore_path,
                                            struct TariPrivateKey *secret_key);

void comms_config_destroy(struct TariCommsConfig *wc);

/// -------------------------------- TariWallet ----------------------------------------------- //

struct TariWallet *wallet_create(const struct TariWalletConfig *config);

bool wallet_add_base_node_peer(struct TariWallet *wallet, struct TariPublicKey *public_key, char *address);

bool wallet_send_ping(struct TariWallet *wallet, struct TariPublicKey *dest_pub_key);

int wallet_get_pings(struct TariWallet *wallet);

void wallet_destroy(struct TariWallet *wallet);

/// -------------------------------- TariKeyManagerSeedWords ----------------------------------------------- ///

struct TariKeyManagerSeedWords *key_manager_seed_words_create(void);

int key_manager_seed_length(const struct TariKeyManagerSeedWords *vec);

bool key_manager_seed_words_add(const char *s, struct TariKeyManagerSeedWords *mgr);

void key_manager_seed_words_destroy(struct TariKeyManagerSeedWords *obj);

const char *key_manager_seed_words_get_at(struct TariKeyManagerSeedWords *mgr, int i);

/// -------------------------------- TariPrivateKey ----------------------------------------------- ///

struct TariPrivateKey *private_key_create(struct ByteVector *bytes);

struct TariPrivateKey *private_key_generate(void);

struct TariPrivateKey *private_key_for_hex(char *key);

struct ByteVector *private_key_get_key(struct TariPrivateKey *pk);

void private_key_destroy(struct TariPrivateKey *pk);

/// -------------------------------- TariPublicKey ----------------------------------------------- ///

struct TariPublicKey *public_key_create(struct ByteVector *bytes);

struct TariPublicKey *public_key_get_from_private_key(struct TariPrivateKey *secret_key);

void public_key_destroy(struct TariPublicKey *pk);

#endif /* wallet_ffi_h */
