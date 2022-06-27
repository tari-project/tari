// Copyright 2022. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * The number of unique fields available. This always matches the number of variants in `OutputField`.
 */
#define OutputFields_NUM_FIELDS 10

enum TariTypeTag {
  String = 0,
  Utxo = 1,
  Commitment = 2,
};

enum TariUtxoSort {
  ValueAsc = 0,
  ValueDesc = 1,
  MinedHeightAsc = 2,
  MinedHeightDesc = 3,
};

/**
 * This struct holds the detailed balance of the Output Manager Service.
 */
struct Balance;

struct ByteVector;

/**
 * # Commitment Signatures
 *
 * Find out more about Commitment signatures [here](https://eprint.iacr.org/2020/061.pdf) and
 * [here](https://documents.uow.edu.au/~wsusilo/ZCMS_IJNS08.pdf).
 *
 * In short, a Commitment Signature is made up of the tuple _(R, u, v)_, where _R_ is a random Pedersen commitment (of
 * two secret nonces) and _u_ and _v_ are the two publicly known private signature keys. It demonstrates ownership of
 * a specific commitment.
 *
 * The Commitment Signature signes a challenge with the value commitment's value and blinding factor. The two nonces
 * should be completely random and never reused - that responsibility lies with the calling function.
 *   C = a*H + x*G          ... (Pedersen commitment to the value 'a' using blinding factor 'x')
 *   R = k_2*H + k_1*G      ... (a public (Pedersen) commitment nonce created with the two random nonces)
 *   u = k_1 + e.x          ... (the first publicly known private key of the signature signing with 'x')
 *   v = k_2 + e.a          ... (the second publicly known private key of the signature signing with 'a')
 *   signature = (R, u, v)  ... (the final signature tuple)
 *
 * Verification of the Commitment Signature (R, u, v) entails the following:
 *   S = v*H + u*G          ... (Pedersen commitment of the publicly known private signature keys)
 *   S =? R + e.C           ... (final verification)
 */
struct CommitmentSignature_RistrettoPublicKey__RistrettoSecretKey;

struct CompletedTransaction;

struct Contact;

struct ContactsLivenessData;

struct Covenant;

struct EmojiSet;

struct FeePerGramStat;

struct FeePerGramStatsResponse;

struct InboundTransaction;

struct OutboundTransaction;

/**
 * Options for UTXO's
 */
struct OutputFeatures;

/**
 * Configuration for a comms node
 */
struct P2pConfig;

/**
 * The [PublicKey](trait.PublicKey.html) implementation for `ristretto255` is a thin wrapper around the dalek
 * library's [RistrettoPoint](struct.RistrettoPoint.html).
 *
 * ## Creating public keys
 * Both [PublicKey](trait.PublicKey.html) and [ByteArray](trait.ByteArray.html) are implemented on
 * `RistrettoPublicKey` so all of the following will work:
 * ```edition2018
 * use rand;
 * use tari_crypto::{
 *     keys::{PublicKey, SecretKey},
 *     ristretto::{RistrettoPublicKey, RistrettoSecretKey},
 * };
 * use tari_utilities::{hex::Hex, ByteArray};
 *
 * let mut rng = rand::thread_rng();
 * let _p1 = RistrettoPublicKey::from_bytes(&[
 *     224, 196, 24, 247, 200, 217, 196, 205, 215, 57, 91, 147, 234, 18, 79, 58, 217, 144, 33, 187, 104, 29, 252, 51,
 *     2, 169, 217, 154, 46, 83, 230, 78,
 * ]);
 * let _p2 = RistrettoPublicKey::from_hex(&"e882b131016b52c1d3337080187cf768423efccbb517bb495ab812c4160ff44e");
 * let sk = RistrettoSecretKey::random(&mut rng);
 * let _p3 = RistrettoPublicKey::from_secret_key(&sk);
 * ```
 */
struct RistrettoPublicKey;

/**
 * The [SecretKey](trait.SecretKey.html) implementation for [Ristretto](https://ristretto.group) is a thin wrapper
 * around the Dalek [Scalar](struct.Scalar.html) type, representing a 256-bit integer (mod the group order).
 *
 * ## Creating secret keys
 * [ByteArray](trait.ByteArray.html) and [SecretKeyFactory](trait.SecretKeyFactory.html) are implemented for
 * [SecretKey](struct .SecretKey.html), so any of the following work (note that hex strings and byte array are
 * little-endian):
 *
 * ```edition2018
 * use rand;
 * use tari_crypto::{keys::SecretKey, ristretto::RistrettoSecretKey};
 * use tari_utilities::{hex::Hex, ByteArray};
 *
 * let mut rng = rand::thread_rng();
 * let _k1 = RistrettoSecretKey::from_bytes(&[
 *     1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
 * ]);
 * let _k2 = RistrettoSecretKey::from_hex(&"100000002000000030000000040000000");
 * let _k3 = RistrettoSecretKey::random(&mut rng);
 * ```
 */
struct RistrettoSecretKey;

struct TariCompletedTransactions;

struct TariContacts;

struct TariPendingInboundTransactions;

struct TariPendingOutboundTransactions;

struct TariPublicKeys;

struct TariSeedWords;

struct TariWallet;

/**
 * The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
 * why it is necessary, refer to the
 * [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
 * The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
 * this transaction can be mined) and the transaction fee, in cleartext.
 */
struct TransactionKernel;

struct TransactionSendStatus;

struct TransportConfig;

typedef struct TransactionKernel TariTransactionKernel;

/**
 * Define the explicit Public key implementation for the Tari base layer
 */
typedef struct RistrettoPublicKey PublicKey;

typedef PublicKey TariPublicKey;

/**
 * Define the explicit Secret key implementation for the Tari base layer.
 */
typedef struct RistrettoSecretKey PrivateKey;

typedef PrivateKey TariPrivateKey;

/**
 * # A Commitment signature implementation on Ristretto
 *
 * `RistrettoComSig` utilises the [curve25519-dalek](https://github.com/dalek-cryptography/curve25519-dalek1)
 * implementation of `ristretto255` to provide Commitment Signature functionality utlizing Schnorr signatures.
 *
 * ## Examples
 *
 * You can create a `RistrettoComSig` from it's component parts:
 *
 * ```edition2018
 * # use tari_crypto::ristretto::*;
 * # use tari_crypto::keys::*;
 * # use tari_crypto::commitment::HomomorphicCommitment;
 * # use tari_utilities::ByteArray;
 * # use tari_utilities::hex::Hex;
 *
 * let r_pub =
 *     HomomorphicCommitment::from_hex("8063d85e151abee630e643e2b3dc47bfaeb8aa859c9d10d60847985f286aad19").unwrap();
 * let u = RistrettoSecretKey::from_bytes(b"10000000000000000000000010000000").unwrap();
 * let v = RistrettoSecretKey::from_bytes(b"a00000000000000000000000a0000000").unwrap();
 * let sig = RistrettoComSig::new(r_pub, u, v);
 * ```
 *
 * or you can create a signature for a commitment by signing a message with knowledge of the commitment and then
 * verify it by calling the `verify_challenge` method:
 *
 * ```rust
 * # use tari_crypto::ristretto::*;
 * # use tari_crypto::keys::*;
 * # use tari_crypto::common::*;
 * # use digest::Digest;
 * # use tari_crypto::commitment::HomomorphicCommitmentFactory;
 * # use tari_crypto::ristretto::pedersen::*;
 *
 * let mut rng = rand::thread_rng();
 * let a_val = RistrettoSecretKey::random(&mut rng);
 * let x_val = RistrettoSecretKey::random(&mut rng);
 * let a_nonce = RistrettoSecretKey::random(&mut rng);
 * let x_nonce = RistrettoSecretKey::random(&mut rng);
 * let e = Blake256::digest(b"Maskerade");
 * let factory = PedersenCommitmentFactory::default();
 * let commitment = factory.commit(&x_val, &a_val);
 * let sig = RistrettoComSig::sign(&a_val, &x_val, &a_nonce, &x_nonce, &e, &factory).unwrap();
 * assert!(sig.verify_challenge(&commitment, &e, &factory));
 * ```
 *
 * # Verifying signatures
 *
 * Given a signature, (R,u,v), a commitment C and a Challenge, e, you can verify that the signature is valid by
 * calling the `verify_challenge` method:
 *
 * ```edition2018
 * # use tari_crypto::ristretto::*;
 * # use tari_crypto::keys::*;
 * # use tari_crypto::commitment::HomomorphicCommitment;
 * # use tari_crypto::ristretto::pedersen::*;
 * # use tari_crypto::common::*;
 * # use tari_utilities::hex::*;
 * # use tari_utilities::ByteArray;
 * # use digest::Digest;
 *
 * let commitment =
 *     HomomorphicCommitment::from_hex("d6cca5cc4cc302c1854a118221d6cf64d100b7da76665dae5199368f3703c665").unwrap();
 * let r_nonce =
 *     HomomorphicCommitment::from_hex("9607f72d84d704825864a4455c2325509ecc290eb9419bbce7ff05f1f578284c").unwrap();
 * let u = RistrettoSecretKey::from_hex("0fd60e6479507fec35a46d2ec9da0ae300e9202e613e99b8f2b01d7ef6eccc02").unwrap();
 * let v = RistrettoSecretKey::from_hex("9ae6621dd99ecc252b90a0eb69577c6f3d2e1e8abcdd43bfd0297afadf95fb0b").unwrap();
 * let sig = RistrettoComSig::new(r_nonce, u, v);
 * let e = Blake256::digest(b"Maskerade");
 * let factory = PedersenCommitmentFactory::default();
 * assert!(sig.verify_challenge(&commitment, &e, &factory));
 * ```
 */
typedef struct CommitmentSignature_RistrettoPublicKey__RistrettoSecretKey RistrettoComSig;

/**
 * Define the explicit Commitment Signature implementation for the Tari base layer.
 */
typedef RistrettoComSig ComSignature;

typedef ComSignature TariCommitmentSignature;

typedef struct Covenant TariCovenant;

typedef struct OutputFeatures TariOutputFeatures;

typedef struct Contact TariContact;

typedef struct ContactsLivenessData TariContactsLivenessData;

typedef struct CompletedTransaction TariCompletedTransaction;

typedef struct OutboundTransaction TariPendingOutboundTransaction;

typedef struct InboundTransaction TariPendingInboundTransaction;

typedef struct TransactionSendStatus TariTransactionSendStatus;

typedef struct TransportConfig TariTransportConfig;

typedef struct P2pConfig TariCommsConfig;

typedef struct Balance TariBalance;

struct TariUtxo {
  char *commitment;
  uint64_t value;
};

struct TariOutputs {
  uintptr_t len;
  uintptr_t cap;
  struct TariUtxo *ptr;
};

struct TariVector {
  enum TariTypeTag tag;
  uintptr_t len;
  uintptr_t cap;
  void *ptr;
};

typedef struct FeePerGramStatsResponse TariFeePerGramStats;

typedef struct FeePerGramStat TariFeePerGramStat;

/**
 * -------------------------------- Strings ------------------------------------------------ ///
 * Frees memory for a char array
 *
 * ## Arguments
 * `ptr` - The pointer to be freed
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C.
 *
 * # Safety
 * None
 */
void string_destroy(char *ptr);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- Transaction Kernel ------------------------------------- ///
 * Gets the excess for a TariTransactionKernel
 *
 * ## Arguments
 * `x` - The pointer to a  TariTransactionKernel
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array. Note that it returns empty if there
 * was an error
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
char *transaction_kernel_get_excess_hex(TariTransactionKernel *kernel,
                                        int *error_out);

/**
 * Gets the public nonce for a TariTransactionKernel
 *
 * ## Arguments
 * `x` - The pointer to a  TariTransactionKernel
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array. Note that it returns empty if there
 * was an error
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
char *transaction_kernel_get_excess_public_nonce_hex(TariTransactionKernel *kernel,
                                                     int *error_out);

/**
 * Gets the signature for a TariTransactionKernel
 *
 * ## Arguments
 * `x` - The pointer to a TariTransactionKernel
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array. Note that it returns empty if there
 * was an error
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
char *transaction_kernel_get_excess_signature_hex(TariTransactionKernel *kernel,
                                                  int *error_out);

/**
 * Frees memory for a TariTransactionKernel
 *
 * ## Arguments
 * `x` - The pointer to a  TariTransactionKernel
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void transaction_kernel_destroy(TariTransactionKernel *x);

/**
 * -------------------------------------------------------------------------------------------- ///
 * -------------------------------- ByteVector ------------------------------------------------ ///
 * Creates a ByteVector
 *
 * ## Arguments
 * `byte_array` - The pointer to the byte array
 * `element_count` - The number of elements in byte_array
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut ByteVector` - Pointer to the created ByteVector. Note that it will be ptr::null_mut()
 * if the byte_array pointer was null or if the elements in the byte_vector don't match
 * element_count when it is created
 *
 * # Safety
 * The ```byte_vector_destroy``` function must be called when finished with a ByteVector to prevent a memory leak
 */
struct ByteVector *byte_vector_create(const unsigned char *byte_array,
                                      unsigned int element_count,
                                      int *error_out);

/**
 * Frees memory for a ByteVector
 *
 * ## Arguments
 * `bytes` - The pointer to a ByteVector
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void byte_vector_destroy(struct ByteVector *bytes);

/**
 * Gets a c_uchar at position in a ByteVector
 *
 * ## Arguments
 * `ptr` - The pointer to a ByteVector
 * `position` - The integer position
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uchar` - Returns a character. Note that the character will be a null terminator (0) if ptr
 * is null or if the position is invalid
 *
 * # Safety
 * None
 */
unsigned char byte_vector_get_at(struct ByteVector *ptr,
                                 unsigned int position,
                                 int *error_out);

/**
 * Gets the number of elements in a ByteVector
 *
 * ## Arguments
 * `ptr` - The pointer to a ByteVector
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uint` - Returns the integer number of elements in the ByteVector. Note that it will be zero
 * if ptr is null
 *
 * # Safety
 * None
 */
unsigned int byte_vector_get_length(const struct ByteVector *vec,
                                    int *error_out);

/**
 * -------------------------------------------------------------------------------------------- ///
 * -------------------------------- Public Key ------------------------------------------------ ///
 * Creates a TariPublicKey from a ByteVector
 *
 * ## Arguments
 * `bytes` - The pointer to a ByteVector
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `TariPublicKey` - Returns a public key. Note that it will be ptr::null_mut() if bytes is null or
 * if there was an error with the contents of bytes
 *
 * # Safety
 * The ```public_key_destroy``` function must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *public_key_create(struct ByteVector *bytes,
                                 int *error_out);

/**
 * Frees memory for a TariPublicKey
 *
 * ## Arguments
 * `pk` - The pointer to a TariPublicKey
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void public_key_destroy(TariPublicKey *pk);

/**
 * Frees memory for TariPublicKeys
 *
 * ## Arguments
 * `pks` - The pointer to TariPublicKeys
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void public_keys_destroy(struct TariPublicKeys *pks);

/**
 * Gets a ByteVector from a TariPublicKey
 *
 * ## Arguments
 * `pk` - The pointer to a TariPublicKey
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut ByteVector` - Returns a pointer to a ByteVector. Note that it returns ptr::null_mut() if pk is null
 *
 * # Safety
 * The ```byte_vector_destroy``` function must be called when finished with the ByteVector to prevent a memory leak.
 */
struct ByteVector *public_key_get_bytes(TariPublicKey *pk,
                                        int *error_out);

/**
 * Creates a TariPublicKey from a TariPrivateKey
 *
 * ## Arguments
 * `secret_key` - The pointer to a TariPrivateKey
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns a pointer to a TariPublicKey
 *
 * # Safety
 * The ```private_key_destroy``` method must be called when finished with a private key to prevent a memory leak
 */
TariPublicKey *public_key_from_private_key(TariPrivateKey *secret_key,
                                           int *error_out);

/**
 * Creates a TariPublicKey from a char array
 *
 * ## Arguments
 * `key` - The pointer to a char array which is hex encoded
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
 * if key is null or if there was an error creating the TariPublicKey from key
 *
 * # Safety
 * The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *public_key_from_hex(const char *key,
                                   int *error_out);

/**
 * Creates a char array from a TariPublicKey in emoji format
 *
 * ## Arguments
 * `pk` - The pointer to a TariPublicKey
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array. Note that it returns empty
 * if emoji is null or if there was an error creating the emoji string from TariPublicKey
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
char *public_key_to_emoji_id(TariPublicKey *pk,
                             int *error_out);

/**
 * Creates a TariPublicKey from a char array in emoji format
 *
 * ## Arguments
 * `const *c_char` - The pointer to a TariPublicKey
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a TariPublicKey. Note that it returns null on error.
 *
 * # Safety
 * The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *emoji_id_to_public_key(const char *emoji,
                                      int *error_out);

/**
 * -------------------------------------------------------------------------------------------- ///
 * -------------------------------- Private Key ----------------------------------------------- ///
 * Creates a TariPrivateKey from a ByteVector
 *
 * ## Arguments
 * `bytes` - The pointer to a ByteVector
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPrivateKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
 * if bytes is null or if there was an error creating the TariPrivateKey from bytes
 *
 * # Safety
 * The ```private_key_destroy``` method must be called when finished with a TariPrivateKey to prevent a memory leak
 */
TariPrivateKey *private_key_create(struct ByteVector *bytes,
                                   int *error_out);

/**
 * Frees memory for a TariPrivateKey
 *
 * ## Arguments
 * `pk` - The pointer to a TariPrivateKey
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void private_key_destroy(TariPrivateKey *pk);

/**
 * Gets a ByteVector from a TariPrivateKey
 *
 * ## Arguments
 * `pk` - The pointer to a TariPrivateKey
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut ByteVectror` - Returns a pointer to a ByteVector. Note that it returns ptr::null_mut()
 * if pk is null
 *
 * # Safety
 * The ```byte_vector_destroy``` must be called when finished with a ByteVector to prevent a memory leak
 */
struct ByteVector *private_key_get_bytes(TariPrivateKey *pk,
                                         int *error_out);

/**
 * Generates a TariPrivateKey
 *
 * ## Arguments
 * `()` - Does  not take any arguments
 *
 * ## Returns
 * `*mut TariPrivateKey` - Returns a pointer to a TariPrivateKey
 *
 * # Safety
 * The ```private_key_destroy``` method must be called when finished with a TariPrivateKey to prevent a memory leak.
 */
TariPrivateKey *private_key_generate(void);

/**
 * Creates a TariPrivateKey from a char array
 *
 * ## Arguments
 * `key` - The pointer to a char array which is hex encoded
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPrivateKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
 * if key is null or if there was an error creating the TariPrivateKey from key
 *
 * # Safety
 * The ```private_key_destroy``` method must be called when finished with a TariPrivateKey to prevent a memory leak
 */
TariPrivateKey *private_key_from_hex(const char *key,
                                     int *error_out);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ------------------------------- Commitment Signature ---------------------------------------///
 * Creates a TariCommitmentSignature from `u`, `v` and `public_nonce` ByteVectors
 *
 * ## Arguments
 * `public_nonce_bytes` - The public nonce signature component as a ByteVector
 * `u_bytes` - The u signature component as a ByteVector
 * `v_bytes` - The v signature component as a ByteVector
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `TariCommitmentSignature` - Returns a commitment signature. Note that it will be ptr::null_mut() if any argument is
 * null or if there was an error with the contents of bytes
 *
 * # Safety
 * The ```commitment_signature_destroy``` function must be called when finished with a TariCommitmentSignature to
 * prevent a memory leak
 */
TariCommitmentSignature *commitment_signature_create_from_bytes(const struct ByteVector *public_nonce_bytes,
                                                                const struct ByteVector *u_bytes,
                                                                const struct ByteVector *v_bytes,
                                                                int *error_out);

/**
 * Frees memory for a TariCommitmentSignature
 *
 * ## Arguments
 * `com_sig` - The pointer to a TariCommitmentSignature
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void commitment_signature_destroy(TariCommitmentSignature *com_sig);

/**
 * -------------------------------------------------------------------------------------------- ///
 * --------------------------------------- Covenant --------------------------------------------///
 * Creates a TariCovenant from a ByteVector containing the covenant bytes
 *
 * ## Arguments
 * `covenant_bytes` - The covenant bytes as a ByteVector
 *
 * ## Returns
 * `TariCovenant` - Returns a commitment signature. Note that it will be ptr::null_mut() if any argument is
 * null or if there was an error with the contents of bytes
 *
 * # Safety
 * The ```covenant_destroy``` function must be called when finished with a TariCovenant to prevent a memory leak
 */
TariCovenant *covenant_create_from_bytes(const struct ByteVector *covenant_bytes,
                                         int *error_out);

/**
 * Frees memory for a TariCovenant
 *
 * ## Arguments
 * `covenant` - The pointer to a TariCovenant
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void covenant_destroy(TariCovenant *covenant);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ---------------------------------- Output Features ------------------------------------------///
 * Creates a TariOutputFeatures from byte values
 *
 * ## Arguments
 * `version` - The encoded value of the version as a byte
 * `flags` - The encoded value of the flags as a byte
 * `maturity` - The encoded value maturity as bytes
 * `recovery_byte` - The encoded value of the recovery byte as a byte
 * `metadata` - The metadata componenet as a ByteVector. It cannot be null
 * `unique_id` - The unique id componenet as a ByteVector. It can be null
 * `mparent_public_key` - The parent public key component as a ByteVector. It can be null
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `TariOutputFeatures` - Returns an output features object. Note that it will be ptr::null_mut() if any mandatory
 * arguments are null or if there was an error with the contents of bytes
 *
 * # Safety
 * The ```output_features_destroy``` function must be called when finished with a TariOutputFeatures to
 * prevent a memory leak
 */
TariOutputFeatures *output_features_create_from_bytes(unsigned char version,
                                                      unsigned char flags,
                                                      unsigned long long maturity,
                                                      unsigned char recovery_byte,
                                                      const struct ByteVector *metadata,
                                                      const struct ByteVector *unique_id,
                                                      const struct ByteVector *parent_public_key,
                                                      int *error_out);

/**
 * Frees memory for a TariOutputFeatures
 *
 * ## Arguments
 * `output_features` - The pointer to a TariOutputFeatures
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void output_features_destroy(TariOutputFeatures *output_features);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- Seed Words ----------------------------------------------///
 * Create an empty instance of TariSeedWords
 *
 * ## Arguments
 * None
 *
 * ## Returns
 * `TariSeedWords` - Returns an empty TariSeedWords instance
 *
 * # Safety
 * None
 */
struct TariSeedWords *seed_words_create(void);

/**
 * Create a TariSeedWords instance containing the entire mnemonic wordlist for the requested language
 *
 * ## Arguments
 * `language` - The required language as a string
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `TariSeedWords` - Returns the TariSeedWords instance containing the entire mnemonic wordlist for the
 * requested language.
 *
 * # Safety
 * The `seed_words_destroy` method must be called when finished with a TariSeedWords instance from rust to prevent a
 * memory leak
 */
struct TariSeedWords *seed_words_get_mnemonic_word_list_for_language(const char *language,
                                                                     int *error_out);

/**
 * Gets the length of TariSeedWords
 *
 * ## Arguments
 * `seed_words` - The pointer to a TariSeedWords
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uint` - Returns number of elements in seed_words, zero if seed_words is null
 *
 * # Safety
 * None
 */
unsigned int seed_words_get_length(const struct TariSeedWords *seed_words,
                                   int *error_out);

/**
 * Gets a seed word from TariSeedWords at position
 *
 * ## Arguments
 * `seed_words` - The pointer to a TariSeedWords
 * `position` - The integer position
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array. Note that it returns an empty char array if
 * TariSeedWords collection is null or the position is invalid
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
char *seed_words_get_at(struct TariSeedWords *seed_words,
                        unsigned int position,
                        int *error_out);

/**
 * Add a word to the provided TariSeedWords instance
 *
 * ## Arguments
 * `seed_words` - The pointer to a TariSeedWords
 * `word` - Word to add
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * 'c_uchar' - Returns a u8 version of the `SeedWordPushResult` enum indicating whether the word was not a valid seed
 * word, if the push was successful and whether the push was successful and completed the full Seed Phrase.
 *  `seed_words` is only modified in the event of a `SuccessfulPush`.
 *     '0' -> InvalidSeedWord
 *     '1' -> SuccessfulPush
 *     '2' -> SeedPhraseComplete
 *     '3' -> InvalidSeedPhrase
 *     '4' -> NoLanguageMatch,
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
unsigned char seed_words_push_word(struct TariSeedWords *seed_words,
                                   const char *word,
                                   int *error_out);

/**
 * Frees memory for a TariSeedWords
 *
 * ## Arguments
 * `seed_words` - The pointer to a TariSeedWords
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void seed_words_destroy(struct TariSeedWords *seed_words);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- Contact -------------------------------------------------///
 * Creates a TariContact
 *
 * ## Arguments
 * `alias` - The pointer to a char array
 * `public_key` - The pointer to a TariPublicKey
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariContact` - Returns a pointer to a TariContact. Note that it returns ptr::null_mut()
 * if alias is null or if pk is null
 *
 * # Safety
 * The ```contact_destroy``` method must be called when finished with a TariContact
 */
TariContact *contact_create(const char *alias,
                            TariPublicKey *public_key,
                            int *error_out);

/**
 * Gets the alias of the TariContact
 *
 * ## Arguments
 * `contact` - The pointer to a TariContact
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array. Note that it returns an empty char array if
 * contact is null
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
char *contact_get_alias(TariContact *contact,
                        int *error_out);

/**
 * Gets the TariPublicKey of the TariContact
 *
 * ## Arguments
 * `contact` - The pointer to a TariContact
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns a pointer to a TariPublicKey. Note that it returns
 * ptr::null_mut() if contact is null
 *
 * # Safety
 * The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *contact_get_public_key(TariContact *contact,
                                      int *error_out);

/**
 * Frees memory for a TariContact
 *
 * ## Arguments
 * `contact` - The pointer to a TariContact
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void contact_destroy(TariContact *contact);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- Contacts -------------------------------------------------///
 * Gets the length of TariContacts
 *
 * ## Arguments
 * `contacts` - The pointer to a TariContacts
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uint` - Returns number of elements in , zero if contacts is null
 *
 * # Safety
 * None
 */
unsigned int contacts_get_length(struct TariContacts *contacts,
                                 int *error_out);

/**
 * Gets a TariContact from TariContacts at position
 *
 * ## Arguments
 * `contacts` - The pointer to a TariContacts
 * `position` - The integer position
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariContact` - Returns a TariContact, note that it returns ptr::null_mut() if contacts is
 * null or position is invalid
 *
 * # Safety
 * The ```contact_destroy``` method must be called when finished with a TariContact to prevent a memory leak
 */
TariContact *contacts_get_at(struct TariContacts *contacts,
                             unsigned int position,
                             int *error_out);

/**
 * Frees memory for a TariContacts
 *
 * ## Arguments
 * `contacts` - The pointer to a TariContacts
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void contacts_destroy(struct TariContacts *contacts);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- Contacts Liveness Data ----------------------------------///
 * Gets the public_key from a TariContactsLivenessData
 *
 * ## Arguments
 * `liveness_data` - The pointer to a TariContactsLivenessData
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut() if
 * liveness_data is null.
 *
 * # Safety
 * The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
 * memory leak
 */
TariPublicKey *liveness_data_get_public_key(TariContactsLivenessData *liveness_data,
                                            int *error_out);

/**
 * Gets the latency in milli-seconds (ms) from a TariContactsLivenessData
 *
 * ## Arguments
 * `liveness_data` - The pointer to a TariContactsLivenessData
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_int` - Returns a pointer to a c_int if the optional latency data (in milli-seconds (ms)) exists, with a
 * value of '-1' if it is None. Note that it also returns '-1' if liveness_data is null.
 *
 * # Safety
 * The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
 * memory leak
 */
int liveness_data_get_latency(TariContactsLivenessData *liveness_data,
                              int *error_out);

/**
 * Gets the last_seen time (in local time) from a TariContactsLivenessData
 *
 * ## Arguments
 * `liveness_data` - The pointer to a TariContactsLivenessData
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array if the optional last_seen data exists, with a value of '?' if it
 * is None. Note that it returns ptr::null_mut() if liveness_data is null.
 *
 * # Safety
 * The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
 * memory leak
 */
char *liveness_data_get_last_seen(TariContactsLivenessData *liveness_data,
                                  int *error_out);

/**
 * Gets the message_type (ContactMessageType enum) from a TariContactsLivenessData
 *
 * ## Arguments
 * `liveness_data` - The pointer to a TariContactsLivenessData
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_int` - Returns the status which corresponds to:
 * | Value | Interpretation |
 * |---|---|
 * |  -1 | NullError        |
 * |   0 | Ping             |
 * |   1 | Pong             |
 * |   2 | NoMessage        |
 *
 * # Safety
 * The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
 * memory leak
 */
int liveness_data_get_message_type(TariContactsLivenessData *liveness_data,
                                   int *error_out);

/**
 * Gets the online_status (ContactOnlineStatus enum) from a TariContactsLivenessData
 *
 * ## Arguments
 * `liveness_data` - The pointer to a TariContactsLivenessData
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_int` - Returns the status which corresponds to:
 * | Value | Interpretation |
 * |---|---|
 * |  -1 | NullError        |
 * |   0 | Online           |
 * |   1 | Offline          |
 * |   2 | NeverSeen        |
 *
 * # Safety
 * The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
 * memory leak
 */
int liveness_data_get_online_status(TariContactsLivenessData *liveness_data,
                                    int *error_out);

/**
 * Frees memory for a TariContactsLivenessData
 *
 * ## Arguments
 * `liveness_data` - The pointer to a TariContactsLivenessData
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void liveness_data_destroy(TariContactsLivenessData *liveness_data);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- CompletedTransactions ----------------------------------- ///
 * Gets the length of a TariCompletedTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariCompletedTransactions
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uint` - Returns the number of elements in a TariCompletedTransactions, note that it will be
 * zero if transactions is null
 *
 * # Safety
 * None
 */
unsigned int completed_transactions_get_length(struct TariCompletedTransactions *transactions,
                                               int *error_out);

/**
 * Gets a TariCompletedTransaction from a TariCompletedTransactions at position
 *
 * ## Arguments
 * `transactions` - The pointer to a TariCompletedTransactions
 * `position` - The integer position
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariCompletedTransaction` - Returns a pointer to a TariCompletedTransaction,
 * note that ptr::null_mut() is returned if transactions is null or position is invalid
 *
 * # Safety
 * The ```completed_transaction_destroy``` method must be called when finished with a TariCompletedTransaction to
 * prevent a memory leak
 */
TariCompletedTransaction *completed_transactions_get_at(struct TariCompletedTransactions *transactions,
                                                        unsigned int position,
                                                        int *error_out);

/**
 * Frees memory for a TariCompletedTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariCompletedTransaction
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void completed_transactions_destroy(struct TariCompletedTransactions *transactions);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- OutboundTransactions ------------------------------------ ///
 * Gets the length of a TariPendingOutboundTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariPendingOutboundTransactions
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uint` - Returns the number of elements in a TariPendingOutboundTransactions, note that it will be
 * zero if transactions is null
 *
 * # Safety
 * None
 */
unsigned int pending_outbound_transactions_get_length(struct TariPendingOutboundTransactions *transactions,
                                                      int *error_out);

/**
 * Gets a TariPendingOutboundTransaction of a TariPendingOutboundTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariPendingOutboundTransactions
 * `position` - The integer position
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPendingOutboundTransaction` - Returns a pointer to a TariPendingOutboundTransaction,
 * note that ptr::null_mut() is returned if transactions is null or position is invalid
 *
 * # Safety
 * The ```pending_outbound_transaction_destroy``` method must be called when finished with a
 * TariPendingOutboundTransaction to prevent a memory leak
 */
TariPendingOutboundTransaction *pending_outbound_transactions_get_at(struct TariPendingOutboundTransactions *transactions,
                                                                     unsigned int position,
                                                                     int *error_out);

/**
 * Frees memory for a TariPendingOutboundTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariPendingOutboundTransactions
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void pending_outbound_transactions_destroy(struct TariPendingOutboundTransactions *transactions);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- InboundTransactions ------------------------------------- ///
 * Gets the length of a TariPendingInboundTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariPendingInboundTransactions
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uint` - Returns the number of elements in a TariPendingInboundTransactions, note that
 * it will be zero if transactions is null
 *
 * # Safety
 * None
 */
unsigned int pending_inbound_transactions_get_length(struct TariPendingInboundTransactions *transactions,
                                                     int *error_out);

/**
 * Gets a TariPendingInboundTransaction of a TariPendingInboundTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariPendingInboundTransactions
 * `position` - The integer position
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPendingOutboundTransaction` - Returns a pointer to a TariPendingInboundTransaction,
 * note that ptr::null_mut() is returned if transactions is null or position is invalid
 *
 * # Safety
 * The ```pending_inbound_transaction_destroy``` method must be called when finished with a
 * TariPendingOutboundTransaction to prevent a memory leak
 */
TariPendingInboundTransaction *pending_inbound_transactions_get_at(struct TariPendingInboundTransactions *transactions,
                                                                   unsigned int position,
                                                                   int *error_out);

/**
 * Frees memory for a TariPendingInboundTransactions
 *
 * ## Arguments
 * `transactions` - The pointer to a TariPendingInboundTransactions
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void pending_inbound_transactions_destroy(struct TariPendingInboundTransactions *transactions);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- CompletedTransaction ------------------------------------- ///
 * Gets the TransactionID of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the TransactionID, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long completed_transaction_get_transaction_id(TariCompletedTransaction *transaction,
                                                            int *error_out);

/**
 * Gets the destination TariPublicKey of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns the destination TariPublicKey, note that it will be
 * ptr::null_mut() if transaction is null
 *
 * # Safety
 * The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *completed_transaction_get_destination_public_key(TariCompletedTransaction *transaction,
                                                                int *error_out);

/**
 * Gets the TariTransactionKernel of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariTransactionKernel` - Returns the transaction kernel, note that it will be
 * ptr::null_mut() if transaction is null, if the transaction status is Pending, or if the number of kernels is not
 * exactly one.
 *
 * # Safety
 * The ```transaction_kernel_destroy``` method must be called when finished with a TariTransactionKernel to prevent a
 * memory leak
 */
TariTransactionKernel *completed_transaction_get_transaction_kernel(TariCompletedTransaction *transaction,
                                                                    int *error_out);

/**
 * Gets the source TariPublicKey of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns the source TariPublicKey, note that it will be
 * ptr::null_mut() if transaction is null
 *
 * # Safety
 * The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *completed_transaction_get_source_public_key(TariCompletedTransaction *transaction,
                                                           int *error_out);

/**
 * Gets the status of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_int` - Returns the status which corresponds to:
 * | Value | Interpretation |
 * |---|---|
 * |  -1 | TxNullError         |
 * |   0 | Completed           |
 * |   1 | Broadcast           |
 * |   2 | MinedUnconfirmed    |
 * |   3 | Imported            |
 * |   4 | Pending             |
 * |   5 | Coinbase            |
 * |   6 | MinedConfirmed      |
 *
 * # Safety
 * None
 */
int completed_transaction_get_status(TariCompletedTransaction *transaction,
                                     int *error_out);

/**
 * Gets the amount of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long completed_transaction_get_amount(TariCompletedTransaction *transaction,
                                                    int *error_out);

/**
 * Gets the fee of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the fee, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long completed_transaction_get_fee(TariCompletedTransaction *transaction,
                                                 int *error_out);

/**
 * Gets the timestamp of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long completed_transaction_get_timestamp(TariCompletedTransaction *transaction,
                                                       int *error_out);

/**
 * Gets the message of a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
 * to an empty char array if transaction is null
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with string coming from rust to prevent a memory leak
 */
const char *completed_transaction_get_message(TariCompletedTransaction *transaction,
                                              int *error_out);

/**
 * This function checks to determine if a TariCompletedTransaction was originally a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `tx` - The TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Returns if the transaction was originally sent from the wallet
 *
 * # Safety
 * None
 */
bool completed_transaction_is_outbound(TariCompletedTransaction *tx,
                                       int *error_out);

/**
 * Gets the number of confirmations of a TariCompletedTransaction
 *
 * ## Arguments
 * `tx` - The TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the number of confirmations of a Completed Transaction
 *
 * # Safety
 * None
 */
unsigned long long completed_transaction_get_confirmations(TariCompletedTransaction *tx,
                                                           int *error_out);

/**
 * Gets the reason a TariCompletedTransaction is cancelled, if it is indeed cancelled
 *
 * ## Arguments
 * `tx` - The TariCompletedTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_int` - Returns the reason for cancellation which corresponds to:
 * | Value | Interpretation |
 * |---|---|
 * |  -1 | Not Cancelled       |
 * |   0 | Unknown             |
 * |   1 | UserCancelled       |
 * |   2 | Timeout             |
 * |   3 | DoubleSpend         |
 * |   4 | Orphan              |
 * |   5 | TimeLocked          |
 * |   6 | InvalidTransaction  |
 * |   7 | AbandonedCoinbase   |
 * # Safety
 * None
 */
int completed_transaction_get_cancellation_reason(TariCompletedTransaction *tx,
                                                  int *error_out);

/**
 * Frees memory for a TariCompletedTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariCompletedTransaction
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void completed_transaction_destroy(TariCompletedTransaction *transaction);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- OutboundTransaction ------------------------------------- ///
 * Gets the TransactionId of a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the TransactionID, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long pending_outbound_transaction_get_transaction_id(TariPendingOutboundTransaction *transaction,
                                                                   int *error_out);

/**
 * Gets the destination TariPublicKey of a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns the destination TariPublicKey, note that it will be
 * ptr::null_mut() if transaction is null
 *
 * # Safety
 * The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *pending_outbound_transaction_get_destination_public_key(TariPendingOutboundTransaction *transaction,
                                                                       int *error_out);

/**
 * Gets the amount of a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long pending_outbound_transaction_get_amount(TariPendingOutboundTransaction *transaction,
                                                           int *error_out);

/**
 * Gets the fee of a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the fee, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long pending_outbound_transaction_get_fee(TariPendingOutboundTransaction *transaction,
                                                        int *error_out);

/**
 * Gets the timestamp of a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long pending_outbound_transaction_get_timestamp(TariPendingOutboundTransaction *transaction,
                                                              int *error_out);

/**
 * Gets the message of a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
 * to an empty char array if transaction is null
 *
 * # Safety
 *  The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory
 * leak
 */
const char *pending_outbound_transaction_get_message(TariPendingOutboundTransaction *transaction,
                                                     int *error_out);

/**
 * Gets the status of a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_int` - Returns the status which corresponds to:
 * | Value | Interpretation |
 * |---|---|
 * |  -1 | TxNullError |
 * |   0 | Completed   |
 * |   1 | Broadcast   |
 * |   2 | Mined       |
 * |   3 | Imported    |
 * |   4 | Pending     |
 *
 * # Safety
 * None
 */
int pending_outbound_transaction_get_status(TariPendingOutboundTransaction *transaction,
                                            int *error_out);

/**
 * Frees memory for a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingOutboundTransaction
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void pending_outbound_transaction_destroy(TariPendingOutboundTransaction *transaction);

/**
 * -------------------------------------------------------------------------------------------- ///
 *
 * ----------------------------------- InboundTransaction ------------------------------------- ///
 * Gets the TransactionId of a TariPendingInboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingInboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the TransactonId, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long pending_inbound_transaction_get_transaction_id(TariPendingInboundTransaction *transaction,
                                                                  int *error_out);

/**
 * Gets the source TariPublicKey of a TariPendingInboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingInboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - Returns a pointer to the source TariPublicKey, note that it will be
 * ptr::null_mut() if transaction is null
 *
 * # Safety
 *  The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *pending_inbound_transaction_get_source_public_key(TariPendingInboundTransaction *transaction,
                                                                 int *error_out);

/**
 * Gets the amount of a TariPendingInboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingInboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long pending_inbound_transaction_get_amount(TariPendingInboundTransaction *transaction,
                                                          int *error_out);

/**
 * Gets the timestamp of a TariPendingInboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingInboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
 *
 * # Safety
 * None
 */
unsigned long long pending_inbound_transaction_get_timestamp(TariPendingInboundTransaction *transaction,
                                                             int *error_out);

/**
 * Gets the message of a TariPendingInboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingInboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
 * to an empty char array if transaction is null
 *
 * # Safety
 *  The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory
 * leak
 */
const char *pending_inbound_transaction_get_message(TariPendingInboundTransaction *transaction,
                                                    int *error_out);

/**
 * Gets the status of a TariPendingInboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingInboundTransaction
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_int` - Returns the status which corresponds to:
 * | Value | Interpretation |
 * |---|---|
 * |  -1 | TxNullError |
 * |   0 | Completed   |
 * |   1 | Broadcast   |
 * |   2 | Mined       |
 * |   3 | Imported    |
 * |   4 | Pending     |
 *
 * # Safety
 * None
 */
int pending_inbound_transaction_get_status(TariPendingInboundTransaction *transaction,
                                           int *error_out);

/**
 * Frees memory for a TariPendingInboundTransaction
 *
 * ## Arguments
 * `transaction` - The pointer to a TariPendingInboundTransaction
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void pending_inbound_transaction_destroy(TariPendingInboundTransaction *transaction);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- Transport Send Status -----------------------------------///
 * Decode the transaction send status of a TariTransactionSendStatus
 *
 * ## Arguments
 * `status` - The pointer to a TariTransactionSendStatus
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_uint` - Returns
 *     !direct_send & !saf_send &  queued   = 0
 *      direct_send &  saf_send & !queued   = 1
 *      direct_send & !saf_send & !queued   = 2
 *     !direct_send &  saf_send & !queued   = 3
 *     any other combination (is not valid) = 4
 *
 * # Safety
 * None
 */
unsigned int transaction_send_status_decode(const TariTransactionSendStatus *status,
                                            int *error_out);

/**
 * Frees memory for a TariTransactionSendStatus
 *
 * ## Arguments
 * `status` - The pointer to a TariPendingInboundTransaction
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void transaction_send_status_destroy(TariTransactionSendStatus *status);

/**
 * -------------------------------------------------------------------------------------------- ///
 * ----------------------------------- Transport Types -----------------------------------------///
 * Creates a memory transport type
 *
 * ## Arguments
 * `()` - Does not take any arguments
 *
 * ## Returns
 * `*mut TariTransportConfig` - Returns a pointer to a memory TariTransportConfig
 *
 * # Safety
 * The ```transport_type_destroy``` method must be called when finished with a TariTransportConfig to prevent a memory
 * leak
 */
TariTransportConfig *transport_memory_create(void);

/**
 * Creates a tcp transport type
 *
 * ## Arguments
 * `listener_address` - The pointer to a char array
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariTransportConfig` - Returns a pointer to a tcp TariTransportConfig, null on error.
 *
 * # Safety
 * The ```transport_type_destroy``` method must be called when finished with a TariTransportConfig to prevent a memory
 * leak
 */
TariTransportConfig *transport_tcp_create(const char *listener_address,
                                          int *error_out);

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
 * `*mut TariTransportConfig` - Returns a pointer to a tor TariTransportConfig, null on error.
 *
 * # Safety
 * The ```transport_config_destroy``` method must be called when finished with a TariTransportConfig to prevent a
 * memory leak
 */
TariTransportConfig *transport_tor_create(const char *control_server_address,
                                          const struct ByteVector *tor_cookie,
                                          unsigned short tor_port,
                                          bool tor_proxy_bypass_for_outbound,
                                          const char *socks_username,
                                          const char *socks_password,
                                          int *error_out);

/**
 * Gets the address for a memory transport type
 *
 * ## Arguments
 * `transport` - Pointer to a TariTransportConfig
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_char` - Returns the address as a pointer to a char array, array will be empty on error
 *
 * # Safety
 * Can only be used with a memory transport type, will crash otherwise
 */
char *transport_memory_get_address(const TariTransportConfig *transport,
                                   int *error_out);

/**
 * Frees memory for a TariTransportConfig
 *
 * ## Arguments
 * `transport` - The pointer to a TariTransportConfig
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 */
void transport_type_destroy(TariTransportConfig *transport);

/**
 * Frees memory for a TariTransportConfig
 *
 * ## Arguments
 * `transport` - The pointer to a TariTransportConfig
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 */
void transport_config_destroy(TariTransportConfig *transport);

/**
 * ---------------------------------------------------------------------------------------------///
 * ----------------------------------- CommsConfig ---------------------------------------------///
 * Creates a TariCommsConfig. The result from this function is required when initializing a TariWallet.
 *
 * ## Arguments
 * `public_address` - The public address char array pointer. This is the address that the wallet advertises publicly to
 * peers
 * `transport` - TariTransportConfig that specifies the type of comms transport to be used.
 * connections are moved to after initial connection. Default if null is 0.0.0.0:7898 which will accept connections
 * from all IP address on port 7898
 * `database_name` - The database name char array pointer. This is the unique name of this
 * wallet's database
 * `database_path` - The database path char array pointer which. This is the folder path where the
 * database files will be created and the application has write access to
 * `discovery_timeout_in_secs`: specify how long the Discovery Timeout for the wallet is.
 * `network`: name of network to connect to. Valid values are: dibbler, igor, localnet, mainnet
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariCommsConfig` - Returns a pointer to a TariCommsConfig, if any of the parameters are
 * null or a problem is encountered when constructing the NetAddress a ptr::null_mut() is returned
 *
 * # Safety
 * The ```comms_config_destroy``` method must be called when finished with a TariCommsConfig to prevent a memory leak
 */
TariCommsConfig *comms_config_create(const char *public_address,
                                     const TariTransportConfig *transport,
                                     const char *database_name,
                                     const char *datastore_path,
                                     unsigned long long discovery_timeout_in_secs,
                                     unsigned long long saf_message_duration_in_secs,
                                     int *error_out);

/**
 * Frees memory for a TariCommsConfig
 *
 * ## Arguments
 * `wc` - The TariCommsConfig pointer
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void comms_config_destroy(TariCommsConfig *wc);

/**
 * This function lists the public keys of all connected peers
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `TariPublicKeys` -  Returns a list of connected public keys. Note the result will be null if there was an error
 *
 * # Safety
 * The caller is responsible for null checking and deallocating the returned object using public_keys_destroy.
 */
struct TariPublicKeys *comms_list_connected_public_keys(struct TariWallet *wallet,
                                                        int *error_out);

/**
 * Creates a TariWallet
 *
 * ## Arguments
 * `config` - The TariCommsConfig pointer
 * `log_path` - An optional file path to the file where the logs will be written. If no log is required pass *null*
 * pointer.
 * `num_rolling_log_files` - Specifies how many rolling log files to produce, if no rolling files are wanted then set
 * this to 0
 * `size_per_log_file_bytes` - Specifies the size, in bytes, at which the logs files will roll over, if no
 * rolling files are wanted then set this to 0
 * `passphrase` - An optional string that represents the passphrase used to
 * encrypt/decrypt the databases for this wallet. If it is left Null no encryption is used. If the databases have been
 * encrypted then the correct passphrase is required or this function will fail.
 * `seed_words` - An optional instance of TariSeedWords, used to create a wallet for recovery purposes.
 * If this is null, then a new master key is created for the wallet.
 * `callback_received_transaction` - The callback function pointer matching the function signature. This will be
 * called when an inbound transaction is received.
 * `callback_received_transaction_reply` - The callback function
 * pointer matching the function signature. This will be called when a reply is received for a pending outbound
 * transaction
 * `callback_received_finalized_transaction` - The callback function pointer matching the function
 * signature. This will be called when a Finalized version on an Inbound transaction is received
 * `callback_transaction_broadcast` - The callback function pointer matching the function signature. This will be
 * called when a Finalized transaction is detected a Broadcast to a base node mempool.
 * `callback_transaction_mined` - The callback function pointer matching the function signature. This will be called
 * when a Broadcast transaction is detected as mined AND confirmed.
 * `callback_transaction_mined_unconfirmed` - The callback function pointer matching the function signature. This will
 * be called when a Broadcast transaction is detected as mined but not yet confirmed.
 * `callback_faux_transaction_confirmed` - The callback function pointer matching the function signature. This will be
 * called when a one-sided transaction is detected as mined AND confirmed.
 * `callback_faux_transaction_unconfirmed` - The callback function pointer matching the function signature. This
 * will be called when a one-sided transaction is detected as mined but not yet confirmed.
 * `callback_transaction_send_result` - The callback function pointer matching the function signature. This is called
 * when a transaction send is completed. The first parameter is the transaction id and the second contains the
 * transaction send status, weather it was send direct and/or send via saf on the one hand or queued for further retry
 * sending on the other hand.
 *     !direct_send & !saf_send &  queued   = 0
 *      direct_send &  saf_send & !queued   = 1
 *      direct_send & !saf_send & !queued   = 2
 *     !direct_send &  saf_send & !queued   = 3
 *     any other combination (is not valid) = 4
 * `callback_transaction_cancellation` - The callback function pointer matching
 * the function signature. This is called when a transaction is cancelled. The first parameter is a pointer to the
 * cancelled transaction, the second is a reason as to why said transaction failed that is mapped to the
 * `TxCancellationReason` enum: pub enum TxCancellationReason {
 *     Unknown,                // 0
 *     UserCancelled,          // 1
 *     Timeout,                // 2
 *     DoubleSpend,            // 3
 *     Orphan,                 // 4
 *     TimeLocked,             // 5
 *     InvalidTransaction,     // 6
 * }
 * `callback_txo_validation_complete` - The callback function pointer matching the function signature. This is called
 * when a TXO validation process is completed. The request_key is used to identify which request this
 * callback references and the second parameter is a is a bool that returns if the validation was successful or not.
 * `callback_contacts_liveness_data_updated` - The callback function pointer matching the function signature. This is
 * called when a contact's liveness status changed. The data represents the contact's updated status information.
 * `callback_balance_updated` - The callback function pointer matching the function signature. This is called whenever
 * the balance changes.
 * `callback_transaction_validation_complete` - The callback function pointer matching the function signature. This is
 * called when a Transaction validation process is completed. The request_key is used to identify which request this
 * callback references and the second parameter is a bool that returns if the validation was successful or not.
 * `callback_saf_message_received` - The callback function pointer that will be called when the Dht has determined that
 * is has connected to enough of its neighbours to be confident that it has received any SAF messages that were waiting
 * for it.
 * `callback_connectivity_status` -  This callback is called when the status of connection to the set base node
 * changes. it will return an enum encoded as an integer as follows:
 * pub enum OnlineStatus {
 *     Connecting,     // 0
 *     Online,         // 1
 *     Offline,        // 2
 * }
 * `recovery_in_progress` - Pointer to an bool which will be modified to indicate if there is an outstanding recovery
 * that should be completed or not to an error code should one occur, may not be null. Functions as an out parameter.
 * `error_out` - Pointer to an int which will be modified
 * to an error code should one occur, may not be null. Functions as an out parameter.
 * ## Returns
 * `*mut TariWallet` - Returns a pointer to a TariWallet, note that it returns ptr::null_mut()
 * if config is null, a wallet error was encountered or if the runtime could not be created
 *
 * # Safety
 * The ```wallet_destroy``` method must be called when finished with a TariWallet to prevent a memory leak
 */
struct TariWallet *wallet_create(TariCommsConfig *config,
                                 const char *log_path,
                                 unsigned int num_rolling_log_files,
                                 unsigned int size_per_log_file_bytes,
                                 const char *passphrase,
                                 const struct TariSeedWords *seed_words,
                                 const char *network_str,
                                 void (*callback_received_transaction)(TariPendingInboundTransaction*),
                                 void (*callback_received_transaction_reply)(TariCompletedTransaction*),
                                 void (*callback_received_finalized_transaction)(TariCompletedTransaction*),
                                 void (*callback_transaction_broadcast)(TariCompletedTransaction*),
                                 void (*callback_transaction_mined)(TariCompletedTransaction*),
                                 void (*callback_transaction_mined_unconfirmed)(TariCompletedTransaction*, uint64_t),
                                 void (*callback_faux_transaction_confirmed)(TariCompletedTransaction*),
                                 void (*callback_faux_transaction_unconfirmed)(TariCompletedTransaction*, uint64_t),
                                 void (*callback_transaction_send_result)(unsigned long long, TariTransactionSendStatus*),
                                 void (*callback_transaction_cancellation)(TariCompletedTransaction*, uint64_t),
                                 void (*callback_txo_validation_complete)(uint64_t, bool),
                                 void (*callback_contacts_liveness_data_updated)(TariContactsLivenessData*),
                                 void (*callback_balance_updated)(TariBalance*),
                                 void (*callback_transaction_validation_complete)(uint64_t, bool),
                                 void (*callback_saf_messages_received)(void),
                                 void (*callback_connectivity_status)(uint64_t),
                                 bool *recovery_in_progress,
                                 int *error_out);

/**
 * Retrieves the balance from a wallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 * ## Returns
 * `*mut Balance` - Returns the pointer to the TariBalance or null if error occurs
 *
 * # Safety
 * The ```balance_destroy``` method must be called when finished with a TariBalance to prevent a memory leak
 */
TariBalance *wallet_get_balance(struct TariWallet *wallet,
                                int *error_out);

/**
 * This function returns a list of unspent UTXO values and commitments.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer,
 * `page` - Page offset,
 * `page_size` - A number of items per page,
 * `sorting` - An enum representing desired sorting,
 * `dust_threshold` - A value filtering threshold. Outputs whose values are <= `dust_threshold` are not listed in the
 * result.
 * `error_out` - A pointer to an int which will be modified to an error
 * code should one occur, may not be null. Functions as an out parameter.
 *
 * ## Returns
 * `*mut TariOutputs` - Returns a struct with an array pointer, length and capacity (needed for proper destruction
 * after use).
 *
 * # Safety
 * `destroy_tari_outputs()` must be called after use.
 * Items that fail to produce `.as_transaction_output()` are omitted from the list and a `warn!()` message is logged to
 * LOG_TARGET.
 */
struct TariOutputs *wallet_get_utxos(struct TariWallet *wallet,
                                     uintptr_t page,
                                     uintptr_t page_size,
                                     enum TariUtxoSort sorting,
                                     uint64_t dust_threshold,
                                     int32_t *error_ptr);

/**
 * Frees memory for a `TariOutputs`
 *
 * ## Arguments
 * `x` - The pointer to `TariOutputs`
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_tari_outputs(struct TariOutputs *x);

/**
 * Frees memory for a `TariVector`
 *
 * ## Arguments
 * `x` - The pointer to `TariVector`
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void destroy_tari_vector(struct TariVector *x);

/**
 * This function will tell the wallet to do a coin split.
 *
 * ## Arguments
 * * `wallet` - The TariWallet pointer
 * * `commitments` - A `TariVector` of "strings", tagged as `TariTypeTag::String`, containing commitment's hex values
 *   (see `Commitment::to_hex()`)
 * * `amount_per_split` - The amount to split
 * * `number_of_splits` - The number of times to split the amount
 * * `fee_per_gram` - The transaction fee
 * * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
 *   Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns the transaction id.
 *
 * # Safety
 * None
 */
uint64_t wallet_coin_split(struct TariWallet *wallet,
                           struct TariVector *commitments,
                           uint64_t amount_per_split,
                           uintptr_t number_of_splits,
                           uint64_t fee_per_gram,
                           int32_t *error_ptr);

uint64_t wallet_coin_join(struct TariWallet *wallet,
                          struct TariVector *commitments,
                          uint64_t fee_per_gram,
                          int32_t *error_ptr);

/**
 * Signs a message using the public key of the TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `msg` - The message pointer.
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 * ## Returns
 * `*mut c_char` - Returns the pointer to the hexadecimal representation of the signature and
 * public nonce, seperated by a pipe character. Empty if an error occured.
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory leak
 */
char *wallet_sign_message(struct TariWallet *wallet,
                          const char *msg,
                          int *error_out);

/**
 * Verifies the signature of the message signed by a TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `public_key` - The pointer to the TariPublicKey of the wallet which originally signed the message
 * `hex_sig_nonce` - The pointer to the sting containing the hexadecimal representation of the
 * signature and public nonce seperated by a pipe character.
 * `msg` - The pointer to the msg the signature will be checked against.
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 * ## Returns
 * `bool` - Returns if the signature is valid or not, will be false if an error occurs.
 *
 * # Safety
 * None
 */
bool wallet_verify_message_signature(struct TariWallet *wallet,
                                     TariPublicKey *public_key,
                                     const char *hex_sig_nonce,
                                     const char *msg,
                                     int *error_out);

/**
 * Adds a base node peer to the TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `public_key` - The TariPublicKey pointer
 * `address` - The pointer to a char array
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Returns if successful or not
 *
 * # Safety
 * None
 */
bool wallet_add_base_node_peer(struct TariWallet *wallet,
                               TariPublicKey *public_key,
                               const char *address,
                               int *error_out);

/**
 * Upserts a TariContact to the TariWallet. If the contact does not exist it will be Inserted. If it does exist the
 * Alias will be updated.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `contact` - The TariContact pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Returns if successful or not
 *
 * # Safety
 * None
 */
bool wallet_upsert_contact(struct TariWallet *wallet,
                           TariContact *contact,
                           int *error_out);

/**
 * Removes a TariContact from the TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `tx` - The TariPendingInboundTransaction pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Returns if successful or not
 *
 * # Safety
 * None
 */
bool wallet_remove_contact(struct TariWallet *wallet,
                           TariContact *contact,
                           int *error_out);

/**
 * Gets the available balance from a TariBalance. This is the balance the user can spend.
 *
 * ## Arguments
 * `balance` - The TariBalance pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - The available balance, 0 if wallet is null
 *
 * # Safety
 * None
 */
unsigned long long balance_get_available(TariBalance *balance,
                                         int *error_out);

/**
 * Gets the time locked balance from a TariBalance. This is the balance the user can spend.
 *
 * ## Arguments
 * `balance` - The TariBalance pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - The time locked balance, 0 if wallet is null
 *
 * # Safety
 * None
 */
unsigned long long balance_get_time_locked(TariBalance *balance,
                                           int *error_out);

/**
 * Gets the pending incoming balance from a TariBalance. This is the balance the user can spend.
 *
 * ## Arguments
 * `balance` - The TariBalance pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - The pending incoming, 0 if wallet is null
 *
 * # Safety
 * None
 */
unsigned long long balance_get_pending_incoming(TariBalance *balance,
                                                int *error_out);

/**
 * Gets the pending outgoing balance from a TariBalance. This is the balance the user can spend.
 *
 * ## Arguments
 * `balance` - The TariBalance pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - The pending outgoing balance, 0 if wallet is null
 *
 * # Safety
 * None
 */
unsigned long long balance_get_pending_outgoing(TariBalance *balance,
                                                int *error_out);

/**
 * Frees memory for a TariBalance
 *
 * ## Arguments
 * `balance` - The pointer to a TariBalance
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void balance_destroy(TariBalance *balance);

/**
 * Sends a TariPendingOutboundTransaction
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `dest_public_key` - The TariPublicKey pointer of the peer
 * `amount` - The amount
 * `fee_per_gram` - The transaction fee
 * `message` - The pointer to a char array
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `unsigned long long` - Returns 0 if unsuccessful or the TxId of the sent transaction if successful
 *
 * # Safety
 * None
 */
unsigned long long wallet_send_transaction(struct TariWallet *wallet,
                                           TariPublicKey *dest_public_key,
                                           unsigned long long amount,
                                           unsigned long long fee_per_gram,
                                           const char *message,
                                           bool one_sided,
                                           int *error_out);

/**
 * Gets a fee estimate for an amount
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `amount` - The amount
 * `fee_per_gram` - The fee per gram
 * `num_kernels` - The number of transaction kernels
 * `num_outputs` - The number of outputs
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `unsigned long long` - Returns 0 if unsuccessful or the fee estimate in MicroTari if successful
 *
 * # Safety
 * None
 */
unsigned long long wallet_get_fee_estimate(struct TariWallet *wallet,
                                           unsigned long long amount,
                                           unsigned long long fee_per_gram,
                                           unsigned long long num_kernels,
                                           unsigned long long num_outputs,
                                           int *error_out);

/**
 * Gets the number of mining confirmations required
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `unsigned long long` - Returns the number of confirmations required
 *
 * # Safety
 * None
 */
unsigned long long wallet_get_num_confirmations_required(struct TariWallet *wallet,
                                                         int *error_out);

/**
 * Sets the number of mining confirmations required
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `num` - The number of confirmations to require
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void wallet_set_num_confirmations_required(struct TariWallet *wallet,
                                           unsigned long long num,
                                           int *error_out);

/**
 * Get the TariContacts from a TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariContacts` - returns the contacts, note that it returns ptr::null_mut() if
 * wallet is null
 *
 * # Safety
 * The ```contacts_destroy``` method must be called when finished with a TariContacts to prevent a memory leak
 */
struct TariContacts *wallet_get_contacts(struct TariWallet *wallet,
                                         int *error_out);

/**
 * Get the TariCompletedTransactions from a TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariCompletedTransactions` - returns the transactions, note that it returns ptr::null_mut() if
 * wallet is null or an error is encountered
 *
 * # Safety
 * The ```completed_transactions_destroy``` method must be called when finished with a TariCompletedTransactions to
 * prevent a memory leak
 */
struct TariCompletedTransactions *wallet_get_completed_transactions(struct TariWallet *wallet,
                                                                    int *error_out);

/**
 * Get the TariPendingInboundTransactions from a TariWallet
 *
 * Currently a CompletedTransaction with the Status of Completed and Broadcast is considered Pending by the frontend
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPendingInboundTransactions` - returns the transactions, note that it returns ptr::null_mut() if
 * wallet is null or and error is encountered
 *
 * # Safety
 * The ```pending_inbound_transactions_destroy``` method must be called when finished with a
 * TariPendingInboundTransactions to prevent a memory leak
 */
struct TariPendingInboundTransactions *wallet_get_pending_inbound_transactions(struct TariWallet *wallet,
                                                                               int *error_out);

/**
 * Get the TariPendingOutboundTransactions from a TariWallet
 *
 * Currently a CompletedTransaction with the Status of Completed and Broadcast is considered Pending by the frontend
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPendingOutboundTransactions` - returns the transactions, note that it returns ptr::null_mut() if
 * wallet is null or and error is encountered
 *
 * # Safety
 * The ```pending_outbound_transactions_destroy``` method must be called when finished with a
 * TariPendingOutboundTransactions to prevent a memory leak
 */
struct TariPendingOutboundTransactions *wallet_get_pending_outbound_transactions(struct TariWallet *wallet,
                                                                                 int *error_out);

/**
 * Get the all Cancelled Transactions from a TariWallet. This function will also get cancelled pending inbound and
 * outbound transaction and include them in this list by converting them to CompletedTransactions
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariCompletedTransactions` - returns the transactions, note that it returns ptr::null_mut() if
 * wallet is null or an error is encountered
 *
 * # Safety
 * The ```completed_transactions_destroy``` method must be called when finished with a TariCompletedTransactions to
 * prevent a memory leak
 */
struct TariCompletedTransactions *wallet_get_cancelled_transactions(struct TariWallet *wallet,
                                                                    int *error_out);

/**
 * Get the TariCompletedTransaction from a TariWallet by its' TransactionId
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `transaction_id` - The TransactionId
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariCompletedTransaction` - returns the transaction, note that it returns ptr::null_mut() if
 * wallet is null, an error is encountered or if the transaction is not found
 *
 * # Safety
 * The ```completed_transaction_destroy``` method must be called when finished with a TariCompletedTransaction to
 * prevent a memory leak
 */
TariCompletedTransaction *wallet_get_completed_transaction_by_id(struct TariWallet *wallet,
                                                                 unsigned long long transaction_id,
                                                                 int *error_out);

/**
 * Get the TariPendingInboundTransaction from a TariWallet by its' TransactionId
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `transaction_id` - The TransactionId
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPendingInboundTransaction` - returns the transaction, note that it returns ptr::null_mut() if
 * wallet is null, an error is encountered or if the transaction is not found
 *
 * # Safety
 * The ```pending_inbound_transaction_destroy``` method must be called when finished with a
 * TariPendingInboundTransaction to prevent a memory leak
 */
TariPendingInboundTransaction *wallet_get_pending_inbound_transaction_by_id(struct TariWallet *wallet,
                                                                            unsigned long long transaction_id,
                                                                            int *error_out);

/**
 * Get the TariPendingOutboundTransaction from a TariWallet by its' TransactionId
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `transaction_id` - The TransactionId
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPendingOutboundTransaction` - returns the transaction, note that it returns ptr::null_mut() if
 * wallet is null, an error is encountered or if the transaction is not found
 *
 * # Safety
 * The ```pending_outbound_transaction_destroy``` method must be called when finished with a
 * TariPendingOutboundtransaction to prevent a memory leak
 */
TariPendingOutboundTransaction *wallet_get_pending_outbound_transaction_by_id(struct TariWallet *wallet,
                                                                              unsigned long long transaction_id,
                                                                              int *error_out);

/**
 * Get a Cancelled transaction from a TariWallet by its TransactionId. Pending Inbound or Outbound transaction will be
 * converted to a CompletedTransaction
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `transaction_id` - The TransactionId
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariCompletedTransaction` - returns the transaction, note that it returns ptr::null_mut() if
 * wallet is null, an error is encountered or if the transaction is not found
 *
 * # Safety
 * The ```completed_transaction_destroy``` method must be called when finished with a TariCompletedTransaction to
 * prevent a memory leak
 */
TariCompletedTransaction *wallet_get_cancelled_transaction_by_id(struct TariWallet *wallet,
                                                                 unsigned long long transaction_id,
                                                                 int *error_out);

/**
 * Get the TariPublicKey from a TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariPublicKey` - returns the public key, note that ptr::null_mut() is returned
 * if wc is null
 *
 * # Safety
 * The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
 */
TariPublicKey *wallet_get_public_key(struct TariWallet *wallet,
                                     int *error_out);

/**
 * Import an external UTXO into the wallet as a non-rewindable (i.e. non-recoverable) output. This will add a spendable
 * UTXO (as EncumberedToBeReceived) and create a faux completed transaction to record the event.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `amount` - The value of the UTXO in MicroTari
 * `spending_key` - The private spending key
 * `source_public_key` - The public key of the source of the transaction
 * `features` - Options for an output's structure or use
 * `metadata_signature` - UTXO signature with the script offset private key, k_O
 * `sender_offset_public_key` - Tari script offset pubkey, K_O
 * `script_private_key` - Tari script private key, k_S, is used to create the script signature
 * `covenant` - The covenant that will be executed when spending this output
 * `message` - The message that the transaction will have
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` -  Returns the TransactionID of the generated transaction, note that it will be zero if the
 * transaction is null
 *
 * # Safety
 * None
 */
unsigned long long wallet_import_external_utxo_as_non_rewindable(struct TariWallet *wallet,
                                                                 unsigned long long amount,
                                                                 TariPrivateKey *spending_key,
                                                                 TariPublicKey *source_public_key,
                                                                 TariOutputFeatures *features,
                                                                 TariCommitmentSignature *metadata_signature,
                                                                 TariPublicKey *sender_offset_public_key,
                                                                 TariPrivateKey *script_private_key,
                                                                 TariCovenant *covenant,
                                                                 const char *message,
                                                                 int *error_out);

/**
 * Cancel a Pending Transaction
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `transaction_id` - The TransactionId
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - returns whether the transaction could be cancelled
 *
 * # Safety
 * None
 */
bool wallet_cancel_pending_transaction(struct TariWallet *wallet,
                                       unsigned long long transaction_id,
                                       int *error_out);

/**
 * This function will tell the wallet to query the set base node to confirm the status of transaction outputs
 * (TXOs).
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` -  Returns a unique Request Key that is used to identify which callbacks refer to this specific sync
 * request. Note the result will be 0 if there was an error
 *
 * # Safety
 * None
 */
unsigned long long wallet_start_txo_validation(struct TariWallet *wallet,
                                               int *error_out);

/**
 * This function will tell the wallet to query the set base node to confirm the status of mined transactions.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` -  Returns a unique Request Key that is used to identify which callbacks refer to this specific sync
 * request. Note the result will be 0 if there was an error
 *
 * # Safety
 * None
 */
unsigned long long wallet_start_transaction_validation(struct TariWallet *wallet,
                                                       int *error_out);

/**
 * This function will tell the wallet retart any broadcast protocols for completed transactions. Ideally this should be
 * called after a successfuly Transaction Validation is complete
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` -  Returns a boolean value indicating if the launch was success or not.
 *
 * # Safety
 * None
 */
bool wallet_restart_transaction_broadcast(struct TariWallet *wallet,
                                          int *error_out);

/**
 * Gets the seed words representing the seed private key of the provided `TariWallet`.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariSeedWords` - A collection of the seed words
 *
 * # Safety
 * The ```tari_seed_words_destroy``` method must be called when finished with a
 * TariSeedWords to prevent a memory leak
 */
struct TariSeedWords *wallet_get_seed_words(struct TariWallet *wallet,
                                            int *error_out);

/**
 * Set the power mode of the wallet to Low Power mode which will reduce the amount of network operations the wallet
 * performs to conserve power
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 * # Safety
 * None
 */
void wallet_set_low_power_mode(struct TariWallet *wallet,
                               int *error_out);

/**
 * Set the power mode of the wallet to Normal Power mode which will then use the standard level of network traffic
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 * # Safety
 * None
 */
void wallet_set_normal_power_mode(struct TariWallet *wallet,
                                  int *error_out);

/**
 * Apply encryption to the databases used in this wallet using the provided passphrase. If the databases are already
 * encrypted this function will fail.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `passphrase` - A string that represents the passphrase will be used to encrypt the databases for this
 * wallet. Once encrypted the passphrase will be required to start a wallet using these databases
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 * # Safety
 * None
 */
void wallet_apply_encryption(struct TariWallet *wallet,
                             const char *passphrase,
                             int *error_out);

/**
 * Remove encryption to the databases used in this wallet. If this wallet is currently encrypted this encryption will
 * be removed. If it is not encrypted then this function will still succeed to make the operation idempotent
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 * # Safety
 * None
 */
void wallet_remove_encryption(struct TariWallet *wallet,
                              int *error_out);

/**
 * Set a Key Value in the Wallet storage used for Client Key Value store
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `key` - The pointer to a Utf8 string representing the Key
 * `value` - The pointer to a Utf8 string representing the Value ot be stored
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Return a boolean value indicating the operation's success or failure. The error_ptr will hold the error
 * code if there was a failure
 *
 * # Safety
 * None
 */
bool wallet_set_key_value(struct TariWallet *wallet,
                          const char *key,
                          const char *value,
                          int *error_out);

/**
 * get a stored Value that was previously stored in the Wallet storage used for Client Key Value store
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `key` - The pointer to a Utf8 string representing the Key
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut c_char` - Returns a pointer to a char array of the Value string. Note that it returns an null pointer if an
 * error occured.
 *
 * # Safety
 * The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
 */
char *wallet_get_value(struct TariWallet *wallet,
                       const char *key,
                       int *error_out);

/**
 * Clears a Value for the provided Key Value in the Wallet storage used for Client Key Value store
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `key` - The pointer to a Utf8 string representing the Key
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Return a boolean value indicating the operation's success or failure. The error_ptr will hold the error
 * code if there was a failure
 *
 * # Safety
 * None
 */
bool wallet_clear_value(struct TariWallet *wallet,
                        const char *key,
                        int *error_out);

/**
 * Check if a Wallet has the data of an In Progress Recovery in its database.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Return a boolean value indicating whether there is an in progress recovery or not. An error will also
 * result in a false result.
 *
 * # Safety
 * None
 */
bool wallet_is_recovery_in_progress(struct TariWallet *wallet,
                                    int *error_out);

/**
 * Starts the Wallet recovery process.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `base_node_public_key` - The TariPublicKey pointer of the Base Node the recovery process will use
 * `recovery_progress_callback` - The callback function pointer that will be used to asynchronously communicate
 * progress to the client. The first argument of the callback is an event enum encoded as a u8 as follows:
 * ```
 * enum RecoveryEvent {
 *     ConnectingToBaseNode,       // 0
 *     ConnectedToBaseNode,        // 1
 *     ConnectionToBaseNodeFailed, // 2
 *     Progress,                   // 3
 *     Completed,                  // 4
 *     ScanningRoundFailed,        // 5
 *     RecoveryFailed,             // 6
 * }
 * ```
 * The second and third arguments are u64 values that will contain different information depending on the event
 * that triggered the callback. The meaning of the second and third argument for each event are as follows:
 *     - ConnectingToBaseNode, 0, 0
 *     - ConnectedToBaseNode, 0, 1
 *     - ConnectionToBaseNodeFailed, number of retries, retry limit
 *     - Progress, current block, total number of blocks
 *     - Completed, total number of UTXO's recovered, MicroTari recovered,
 *     - ScanningRoundFailed, number of retries, retry limit
 *     - RecoveryFailed, 0, 0
 *
 * If connection to a base node is successful the flow of callbacks should be:
 *     - The process will start with a callback with `ConnectingToBaseNode` showing a connection is being attempted
 *       this could be repeated multiple times until a connection is made.
 *     - The next a callback with `ConnectedToBaseNode` indicate a successful base node connection and process has
 *       started
 *     - In Progress callbacks will be of the form (n, m) where n < m
 *     - If the process completed successfully then the final `Completed` callback will return how many UTXO's were
 *       scanned and how much MicroTari was recovered
 *     - If there is an error in the connection process then the `ConnectionToBaseNodeFailed` will be returned
 *     - If there is a minor error in scanning then `ScanningRoundFailed` will be returned and another connection/sync
 *       attempt will be made
 *     - If a unrecoverable error occurs the `RecoveryFailed` event will be returned and the client will need to start
 *       a new process.
 *
 * `recovered_output_message` - A string that will be used as the message for any recovered outputs. If Null the
 * default     message will be used
 *
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Return a boolean value indicating whether the process started successfully or not, the process will
 * continue to run asynchronously and communicate it progress via the callback. An error will also produce a false
 * result.
 *
 * # Safety
 * None
 */
bool wallet_start_recovery(struct TariWallet *wallet,
                           TariPublicKey *base_node_public_key,
                           void (*recovery_progress_callback)(uint8_t, uint64_t, uint64_t),
                           const char *recovered_output_message,
                           int *error_out);

/**
 * Set the text message that is applied to a detected One-Side payment transaction when it is scanned from the
 * blockchain
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer.
 * `message` - The pointer to a Utf8 string representing the Message
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `bool` - Return a boolean value indicating the operation's success or failure. The error_ptr will hold the error
 * code if there was a failure
 *
 * # Safety
 * None
 */
bool wallet_set_one_sided_payment_message(struct TariWallet *wallet,
                                          const char *message,
                                          int *error_out);

/**
 * This function will produce a partial backup of the specified wallet database file. This backup will be written to
 * the provided file (full path must include the filename and extension) and will include the full wallet db but will
 * clear the sensitive Master Private Key
 *
 * ## Arguments
 * `original_file_path` - The full path of the original database file to be backed up, including the file name and
 * extension `backup_file_path` - The full path, including the file name and extension, of where the backup db will be
 * written `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
 * Functions as an out parameter.
 *
 * ## Returns
 *
 * # Safety
 * None
 */
void file_partial_backup(const char *original_file_path,
                         const char *backup_file_path,
                         int *error_out);

/**
 * Gets the current emoji set
 *
 * ## Arguments
 * `()` - Does not take any arguments
 *
 * ## Returns
 * `*mut EmojiSet` - Pointer to the created EmojiSet.
 *
 * # Safety
 * The ```emoji_set_destroy``` function must be called when finished with a ByteVector to prevent a memory leak
 */
struct EmojiSet *get_emoji_set(void);

/**
 * Gets the length of the current emoji set
 *
 * ## Arguments
 * `*mut EmojiSet` - Pointer to emoji set
 *
 * ## Returns
 * `c_int` - Pointer to the created EmojiSet.
 *
 * # Safety
 * None
 */
unsigned int emoji_set_get_length(const struct EmojiSet *emoji_set, int *error_out);

/**
 * Gets a ByteVector at position in a EmojiSet
 *
 * ## Arguments
 * `emoji_set` - The pointer to a EmojiSet
 * `position` - The integer position
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `ByteVector` - Returns a ByteVector. Note that the ByteVector will be null if ptr
 * is null or if the position is invalid
 *
 * # Safety
 * The ```byte_vector_destroy``` function must be called when finished with the ByteVector to prevent a memory leak.
 */
struct ByteVector *emoji_set_get_at(const struct EmojiSet *emoji_set,
                                    unsigned int position,
                                    int *error_out);

/**
 * Frees memory for a EmojiSet
 *
 * ## Arguments
 * `emoji_set` - The EmojiSet pointer
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void emoji_set_destroy(struct EmojiSet *emoji_set);

/**
 * Frees memory for a TariWallet
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void wallet_destroy(struct TariWallet *wallet);

/**
 * This function will log the provided string at debug level. To be used to have a client log messages to the LibWallet
 * logs.
 *
 * ## Arguments
 * `msg` - A string that will be logged at the debug level. If msg is null nothing will be done.
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * # Safety
 * None
 */
void log_debug_message(const char *msg,
                       int *error_out);

/**
 * ------------------------------------- FeePerGramStats ------------------------------------ ///
 * Get the TariFeePerGramStats from a TariWallet.
 *
 * ## Arguments
 * `wallet` - The TariWallet pointer
 * `count` - The maximum number of blocks to be checked
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter
 *
 * ## Returns
 * `*mut TariCompletedTransactions` - returns the transactions, note that it returns ptr::null_mut() if
 * wallet is null or an error is encountered.
 *
 * # Safety
 * The ```fee_per_gram_stats_destroy``` method must be called when finished with a TariFeePerGramStats to prevent
 * a memory leak.
 */
TariFeePerGramStats *wallet_get_fee_per_gram_stats(struct TariWallet *wallet,
                                                   unsigned int count,
                                                   int *error_out);

/**
 * Get length of stats from the TariFeePerGramStats.
 *
 * ## Arguments
 * `fee_per_gram_stats` - The pointer to a TariFeePerGramStats
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter
 *
 * ## Returns
 * `c_uint` - length of stats in TariFeePerGramStats
 *
 * # Safety
 * None
 */
unsigned int fee_per_gram_stats_get_length(TariFeePerGramStats *fee_per_gram_stats,
                                           int *error_out);

/**
 * Get TariFeePerGramStat at position from the TariFeePerGramStats.
 *
 * ## Arguments
 * `fee_per_gram_stats` - The pointer to a TariFeePerGramStats.
 * `position` - The integer position.
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `*mut TariCompletedTransactions` - returns the TariFeePerGramStat, note that it returns ptr::null_mut() if
 * fee_per_gram_stats is null or an error is encountered.
 *
 * # Safety
 * The ```fee_per_gram_stat_destroy``` method must be called when finished with a TariCompletedTransactions to 4prevent
 * a memory leak.
 */
TariFeePerGramStat *fee_per_gram_stats_get_at(TariFeePerGramStats *fee_per_gram_stats,
                                              unsigned int position,
                                              int *error_out);

/**
 * Frees memory for a TariFeePerGramStats
 *
 * ## Arguments
 * `fee_per_gram_stats` - The TariFeePerGramStats pointer
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void fee_per_gram_stats_destroy(TariFeePerGramStats *fee_per_gram_stats);

/**
 * ------------------------------------------------------------------------------------------ ///
 * ------------------------------------- FeePerGramStat ------------------------------------- ///
 * Get the order of TariFeePerGramStat
 *
 * ## Arguments
 * `fee_per_gram_stats` - The TariFeePerGramStat pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns order
 *
 * # Safety
 * None
 */
unsigned long long fee_per_gram_stat_get_order(TariFeePerGramStat *fee_per_gram_stat,
                                               int *error_out);

/**
 * Get the minimum fee per gram of TariFeePerGramStat
 *
 * ## Arguments
 * `fee_per_gram_stats` - The TariFeePerGramStat pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns minimum fee per gram
 *
 * # Safety
 * None
 */
unsigned long long fee_per_gram_stat_get_min_fee_per_gram(TariFeePerGramStat *fee_per_gram_stat,
                                                          int *error_out);

/**
 * Get the average fee per gram of TariFeePerGramStat
 *
 * ## Arguments
 * `fee_per_gram_stats` - The TariFeePerGramStat pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns average fee per gram
 *
 * # Safety
 * None
 */
unsigned long long fee_per_gram_stat_get_avg_fee_per_gram(TariFeePerGramStat *fee_per_gram_stat,
                                                          int *error_out);

/**
 * Get the maximum fee per gram of TariFeePerGramStat
 *
 * ## Arguments
 * `fee_per_gram_stats` - The TariFeePerGramStat pointer
 * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
 * as an out parameter.
 *
 * ## Returns
 * `c_ulonglong` - Returns maximum fee per gram
 *
 * # Safety
 * None
 */
unsigned long long fee_per_gram_stat_get_max_fee_per_gram(TariFeePerGramStat *fee_per_gram_stat,
                                                          int *error_out);

/**
 * Frees memory for a TariFeePerGramStat
 *
 * ## Arguments
 * `fee_per_gram_stats` - The TariFeePerGramStat pointer
 *
 * ## Returns
 * `()` - Does not return a value, equivalent to void in C
 *
 * # Safety
 * None
 */
void fee_per_gram_stat_destroy(TariFeePerGramStat *fee_per_gram_stat);
