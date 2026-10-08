// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

#[cfg(any(target_os = "stax", target_os = "flex"))]
use include_gif::include_gif;
use ledger_device_sdk::io::Comm;
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::{Field, NbglGlyph, NbglReview, NbglStatus};
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::{
    bitmaps::{CROSSMARK, EYE, VALIDATE_14},
    gadgets::{Field, MultiFieldReview, SingleMessage},
};

use minotari_ledger_wallet_common::{
    codec::{
        Decode,
        GetRawSchnorrSignatureLegacyNonceRequest,
        GetRawSchnorrSignatureRequest,
        GetScriptSchnorrSignatureRequest,
        SchnorrReply,
    },
    legacy_nonce::{
        check_legacy_account,
        check_legacy_nonce_branches,
        check_legacy_nonce_index,
        check_legacy_nonce_use,
        legacy_account_word,
        legacy_signature_purpose,
        record_legacy_nonce_use,
        LegacyNonceRecordError,
        LegacyNonceUse,
        LEGACY_NONCE_RECORD_SIZE,
    },
    u64_to_string,
};

use crate::{
    alloc::string::ToString,
    branch_key_from_u64,
    crypto::schnorr::SchnorrSignature,
    hash_domain,
    handlers::get_ephemeral_nonce::{nonce_store_error_to_app_sw, EphemeralNonceCtx},
    utils::{derive_from_bip32_key, get_random_nonce, legacy_challenge_hash},
    wire::{invalid_data_length, reply, with_screen},
    AppSW,
    KeyType,
};

hash_domain!(CheckSigHashDomain, "com.tari.script.check_sig", 1);
hash_domain!(SchnorrSigChallenge, "com.tari.schnorr_signature", 1);

/// The type used for `CheckSig`, `CheckMultiSig`, and related opcodes' signatures
pub type CheckSigSchnorrSignature = SchnorrSignature<CheckSigHashDomain>;
pub type RistrettoSchnorr = SchnorrSignature<SchnorrSigChallenge>;

/// The legacy nonce instruction's used-nonce record: which nonce indexes it has signed with, for which key and
/// challenge. See `minotari_ledger_wallet_common::legacy_nonce::LegacyNonceUse`.
///
/// RAM-backed and owned by the event loop for the whole application run; it is never reset by another instruction or
/// by an error, and is cleared only when the application run ends (which the host can cause). It is never persisted:
/// by design nothing is stored on the device. An honest host protects itself across runs; a compromised host that
/// restarts the app between two approvals is stopped only once device-issued one-run handles replace host-chosen
/// nonce indexes (the TODO in `minotari_ledger_wallet_common::legacy_nonce`).
pub type LegacyNonceCtx = [Option<LegacyNonceUse>; LEGACY_NONCE_RECORD_SIZE];

fn legacy_record_error_to_app_sw(e: LegacyNonceRecordError) -> AppSW {
    match e {
        LegacyNonceRecordError::Reused => AppSW::LegacyNonceReused,
        LegacyNonceRecordError::Full => AppSW::LegacyNonceStoreFull,
    }
}

/// Sign a challenge with a device held key and a device generated nonce named by `nonce_handle`.
///
/// The nonce is drawn by `GenerateEphemeralNonce` and only ever named by an opaque handle. That is the whole point
/// of this instruction: with a host chosen nonce, two signatures over different challenges give up the private key
/// as `k = (s1 - s2) / (e1 - e2)`, and the host is free to ask twice.
pub fn handler_get_raw_schnorr_signature(comm: &mut Comm, nonce_ctx: &mut EphemeralNonceCtx) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetRawSchnorrSignatureRequest::decode(data).map_err(|_| invalid_data_length())?;

    // Note: `KeyType::from_branch_key` rejects the spend branch, so the host cannot point this handler at `alpha`.
    let private_key_type = KeyType::from_branch_key(request.branch)?;

    let private_key = derive_from_bip32_key(request.account, request.index, private_key_type)?;

    // Take the nonce out of the store *before* it is used, so that every path from here on - success, a signing
    // failure, or anything a later change adds - leaves the slot empty. A slot that survived a failed signature
    // would be a nonce the host could spend a second time on a different challenge.
    let private_nonce = nonce_ctx.take(request.nonce_handle).map_err(nonce_store_error_to_app_sw)?;

    let signature = match RistrettoSchnorr::sign_raw_uniform(&private_key, private_nonce, request.challenge) {
        Ok(sig) => sig,
        Err(_e) => {
            let error_string = "Invalid Challange".to_string();
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                SingleMessage::new(&format!("Signing error: {}", error_string)).show_and_wait();
            }

            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!("Signing error: {}", error_string))
                    .show(false);
            }
            return Err(AppSW::RawSchnorrSignatureFail);
        },
    };

    reply(
        comm,
        &SchnorrReply::new(
            signature.get_public_nonce().as_array(),
            signature.get_signature().as_array(),
        ),
    );

    Ok(())
}

/// Sign a challenge with a deterministic, host indexed nonce.
///
/// DEPRECATED - DO NOT ADD CALLERS. This is [`handler_get_raw_schnorr_signature`] as it was before nonces moved on
/// to the device, and it carries the flaw that change fixed: the host picks the nonce index, so it can ask for two
/// signatures over the same key and nonce with different challenges and solve for the private key.
///
/// It survives for the pre-mine spend flow alone. `check_legacy_nonce_branches` holds it to `PreMine` keys,
/// `check_legacy_nonce_index` to nonce indexes an older application never derived, and `legacy_nonce_ctx` refuses a
/// second use of a nonce index for anything but the identical request until the application restarts. Every request
/// that passes is shown to the user for approval before anything is signed.
///
/// See `minotari_ledger_wallet_common::legacy_nonce` for the canonical account of what this costs, what it reached
/// before it was narrowed to `PreMine` (`alpha`, via the script offset reply), and the TODO that deletes this handler
/// along with everything else on the legacy path.
pub fn handler_get_raw_schnorr_signature_legacy_nonce(
    comm: &mut Comm,
    legacy_nonce_ctx: &mut LegacyNonceCtx,
) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetRawSchnorrSignatureLegacyNonceRequest::decode(data).map_err(|_| invalid_data_length())?;

    let private_key_branch = branch_key_from_u64(request.key_branch)?;
    let nonce_branch = branch_key_from_u64(request.nonce_branch)?;

    // Signing with a deterministic nonce is equivalent to handing the private key over, so the branches this
    // instruction will touch are held to the ones the pre-mine spend flow actually uses. The whitelist is shared
    // with the host - and unit tested - in `minotari_ledger_wallet_common::legacy_nonce`, so the two cannot drift.
    // This check is the one that counts; the host's is only there to produce a legible error. It runs before the
    // review, so a refused request never reaches the screen.
    check_legacy_nonce_branches(private_key_branch, nonce_branch).map_err(|_| AppSW::BadBranchKey)?;
    // An index below 2^32 names a nonce an application before 6.1.1-pre.0 may already have signed with.
    check_legacy_nonce_index(request.nonce_index).map_err(|_| AppSW::BadBranchKey)?;
    // An account of 2^32 or more derives the same keys and nonces as its low word, and the review does not show the
    // account; refuse the ambiguous form. The record below keys on the low word regardless.
    check_legacy_account(request.account).map_err(|_| AppSW::BadBranchKey)?;

    // Note: `KeyType::from_branch_key` rejects the spend branch a second time, so `alpha` stays unreachable even
    // if the whitelist above is ever loosened.
    let private_key_type = KeyType::from_branch_key(request.key_branch)?;
    let nonce_key_type = KeyType::from_branch_key(request.nonce_branch)?;

    // Everything used after the review is an owned copy, never a borrow of `data`: on Stax and Flex the review
    // polls for events, and an APDU arriving meanwhile overwrites the buffer `request` borrows. See `wire`.
    let account = request.account;
    let key_index = request.key_index;
    let nonce_index = request.nonce_index;
    let challenge: [u8; 64] = *request.challenge;

    // A nonce index may be signed with again only for the identical request. Checked before the review, so a reuse is
    // refused without a screen; recorded after the approval and before the signature, below.
    let use_of_nonce = LegacyNonceUse {
        account: legacy_account_word(account),
        nonce_index,
        key_branch: private_key_branch.as_byte(),
        key_index,
        challenge_hash: legacy_challenge_hash(&challenge),
    };
    check_legacy_nonce_use(&legacy_nonce_ctx[..], &use_of_nonce).map_err(legacy_record_error_to_app_sw)?;

    let purpose = legacy_signature_purpose(key_index);
    let key_value = format!("{} {}", private_key_branch.as_str(), u64_to_string(key_index));
    let nonce_value = format!("{} {}", nonce_branch.as_str(), u64_to_string(nonce_index));
    let fields = [
        Field {
            name: "Purpose",
            value: purpose,
        },
        Field {
            name: "Key",
            value: &key_value,
        },
        Field {
            name: "Nonce",
            value: &nonce_value,
        },
    ];

    #[cfg(not(any(target_os = "stax", target_os = "flex")))]
    {
        let review = MultiFieldReview::new(
            &fields,
            &["Review ", "Transaction"],
            Some(&EYE),
            "Approve",
            Some(&VALIDATE_14),
            "Reject",
            Some(&CROSSMARK),
        );
        if !with_screen(comm, || review.show()) {
            return Err(AppSW::UserCancelled);
        }
    }
    #[cfg(any(target_os = "stax", target_os = "flex"))]
    {
        const TARI: NbglGlyph = NbglGlyph::from_include(include_gif!("key_64x64.gif", NBGL));
        let review: NbglReview = NbglReview::new()
            .titles("Review transaction", "", "Sign transaction")
            .glyph(&TARI);
        if !with_screen(comm, || review.show(&fields)) {
            return Err(AppSW::UserCancelled);
        }
    }

    // Recorded before anything is derived or signed, so a failure from here on still leaves the nonce index spent.
    record_legacy_nonce_use(&mut legacy_nonce_ctx[..], use_of_nonce).map_err(legacy_record_error_to_app_sw)?;

    let private_key = derive_from_bip32_key(account, key_index, private_key_type)?;
    let private_nonce = derive_from_bip32_key(account, nonce_index, nonce_key_type)?;

    let signature = match RistrettoSchnorr::sign_raw_uniform(&private_key, private_nonce, &challenge) {
        Ok(sig) => sig,
        Err(_e) => {
            let error_string = "Invalid Challange".to_string();
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                SingleMessage::new(&format!("Signing error: {}", error_string)).show_and_wait();
            }

            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!("Signing error: {}", error_string))
                    .show(false);
            }
            return Err(AppSW::RawSchnorrSignatureFail);
        },
    };

    reply(
        comm,
        &SchnorrReply::new(
            signature.get_public_nonce().as_array(),
            signature.get_signature().as_array(),
        ),
    );

    Ok(())
}

pub fn handler_get_script_schnorr_signature(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetScriptSchnorrSignatureRequest::decode(data).map_err(|_| invalid_data_length())?;

    let private_key_type = KeyType::from_branch_key(request.branch)?;

    let private_key = derive_from_bip32_key(request.account, request.index, private_key_type)?;

    let random_nonce = get_random_nonce()?.clone();
    let signature =
        match CheckSigSchnorrSignature::sign_with_nonce_and_message(&private_key, random_nonce, request.message) {
            Ok(sig) => sig,
            Err(_e) => {
                let error_string = "Invalid Challange".to_string();
                #[cfg(not(any(target_os = "stax", target_os = "flex")))]
                {
                    SingleMessage::new(&format!("Signing error: {}", error_string)).show_and_wait();
                }

                #[cfg(any(target_os = "stax", target_os = "flex"))]
                {
                    NbglStatus::new()
                        .text(&format!("Signing error: {}", error_string))
                        .show(false);
                }
                return Err(AppSW::SchnorrSignatureFail);
            },
        };
    reply(
        comm,
        &SchnorrReply::new(
            signature.get_public_nonce().as_array(),
            signature.get_signature().as_array(),
        ),
    );

    Ok(())
}
