//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod comms_config;
pub mod ffi_bytes;
pub mod ffi_import;
pub use comms_config::CommsConfig;
mod wallet_address;
pub use wallet_address::WalletAddress;
mod wallet;
pub use wallet::Wallet;
mod public_key;
pub use public_key::PublicKey;
mod public_keys;
pub use public_keys::PublicKeys;
mod private_key;
pub use private_key::PrivateKey;
mod ffi_string;
pub use ffi_string::FFIString;
mod seed_words;
pub use seed_words::SeedWords;
mod contact;
pub use contact::Contact;
mod contacts;
pub use contacts::Contacts;
mod balance;
pub use balance::Balance;
mod vector;
pub use vector::Vector;
mod coin_preview;
pub use coin_preview::CoinPreview;
mod pending_outbound_transactions;
pub use pending_outbound_transactions::PendingOutboundTransactions;
mod pending_outbound_transaction;
pub use pending_outbound_transaction::PendingOutboundTransaction;
mod pending_inbound_transactions;
pub use pending_inbound_transactions::PendingInboundTransactions;
mod pending_inbound_transaction;
pub use pending_inbound_transaction::PendingInboundTransaction;
mod completed_transactions;
pub use completed_transactions::CompletedTransactions;
mod completed_transaction;
pub use completed_transaction::CompletedTransaction;
mod kernel;
pub use kernel::Kernel;
mod callbacks;
pub use callbacks::Callbacks;
mod transaction_send_status;
pub use transaction_send_status::TransactionSendStatus;
mod contacts_liveness_data;
pub use contacts_liveness_data::ContactsLivenessData;
mod fee_per_gram_stats;
pub use fee_per_gram_stats::FeePerGramStats;
mod fee_per_gram_stat;
pub use fee_per_gram_stat::FeePerGramStat;

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use regex::Regex;

    #[derive(Debug)]
    struct FnMap {
        fn_name: String,
        fn_return_type: String,
        fn_args: String,
    }

    fn trim_whitespace(s: &str) -> String {
        let mut value = s.trim().to_string().replace("\r\n", " ").replace("\n", " ");
        while value.contains("  ") {
            value = value.replace("  ", " ");
        }
        value
    }

    fn clean_lib_content(content: &str) -> (usize, String) {
        let mut parsed = String::new();

        let fn_lines = content
            .lines()
            .enumerate()
            .filter(|(_i, line)| line.starts_with("pub unsafe extern \"C\" fn"))
            .map(|(i, _line)| i)
            .collect::<Vec<usize>>();
        let final_fn_line = fn_lines[fn_lines.len() - 1];

        for (count, line) in content.lines().enumerate().skip(fn_lines[0]) {
            if line.contains("mod test {") && count > final_fn_line - (fn_lines[0]) {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() ||
                trimmed.starts_with("#[") ||
                trimmed.starts_with("///") ||
                trimmed.starts_with("//")
            {
                continue;
            }
            parsed.push_str(trimmed);
            parsed.push('\n');
        }

        let mut result = String::new();
        let mut signature_definition = false;
        let mut definition_end = false;
        let mut fn_count = 0;
        for line in parsed.lines() {
            if line.contains("pub unsafe extern") {
                fn_count += 1;
                signature_definition = true;
            }
            if signature_definition && (line.contains(") {") || line.contains(") -> ")) {
                definition_end = true;
            }
            if signature_definition {
                // Replace the '"C"' in the signature with '~C~' to make the regex easier to parse
                let replaced = line
                    .to_string()
                    .replace("pub unsafe extern \"C\" fn", "pub unsafe extern ~C~ fn");
                result.push_str(replaced.as_str());
                if definition_end {
                    result.push('\n');
                    signature_definition = false;
                    definition_end = false;
                } else {
                    result.push(' ');
                }
            }
        }

        (fn_count, result)
    }

    fn clean_ffi_import_content(content: &str) -> (usize, String) {
        let mut parsed = String::new();

        let fn_lines = content
            .lines()
            .enumerate()
            .filter(|(_i, line)| line.contains("pub fn "))
            .map(|(i, _line)| i)
            .collect::<Vec<usize>>();

        for line in content.lines().skip(fn_lines[0]) {
            let trimmed = line.trim();
            if trimmed.is_empty() ||
                trimmed.starts_with("#[") ||
                trimmed.starts_with("///") ||
                trimmed.starts_with("//")
            {
                continue;
            }
            parsed.push_str(trimmed);
            parsed.push('\n');
        }

        let mut result = String::new();
        let mut signature_definition = false;
        let mut definition_end = false;
        let mut fn_count = 0;
        for line in parsed.lines() {
            if line.contains("pub fn ") {
                fn_count += 1;
                signature_definition = true;
            }
            if signature_definition && line.contains(";") {
                definition_end = true;
            }
            if signature_definition {
                // Replace the ";" at the end of the signature with " {" to make the regex easier to parse
                result.push_str(line.to_string().replace(";", " {").as_str());
                if definition_end {
                    result.push('\n');
                    signature_definition = false;
                    definition_end = false;
                } else {
                    result.push(' ');
                }
            }
        }

        (fn_count, result)
    }

    fn parse_function_signatures(content: &str, re: Regex) -> HashMap<String, FnMap> {
        let mut fn_maps = HashMap::new();

        for cap in re.captures_iter(content) {
            let mut fn_map = FnMap {
                fn_name: trim_whitespace(&cap["fn_name"]),
                fn_return_type: trim_whitespace(cap.name("fn_return_type").map_or("", |m| m.as_str())),
                fn_args: trim_whitespace(&cap["fn_args"]),
            };
            if fn_map.fn_args.ends_with(",") {
                fn_map.fn_args.pop();
            }
            fn_map.fn_args = fn_map.fn_args.replace("( ", "(");
            fn_map.fn_args = fn_map.fn_args.replace(" )", ")");
            fn_map.fn_args = fn_map.fn_args.replace(",)", ")");
            assert!(fn_maps.insert(fn_map.fn_name.clone(), fn_map).is_none());
        }

        fn_maps
    }

    #[test]
    fn test_ffi_import_fn_signatures() {
        let ffi_lib_content = fs::read_to_string("../base_layer/wallet_ffi/src/lib.rs").unwrap();
        let (fn_count, cleaned_ffi_lib_content) = clean_lib_content(&ffi_lib_content);
        // 'cleaned_ffi_lib_content' looks like:
        // -------------------------------------
        // pub unsafe extern ~C~ fn create_tari_vector(tag: TariTypeTag) -> *mut TariVector {
        // pub unsafe extern ~C~ fn destroy_tari_vector(v: *mut TariVector) {
        // pub unsafe extern ~C~ fn destroy_tari_coin_preview(p: *mut TariCoinPreview) {
        let re = Regex::new(
            r"(?m)^\s*pub\s+unsafe\s+extern\s+~C~\s+fn\s+(?P<fn_name>\w+)\s*\((?P<fn_args>.*)\)\s*(->\s*(?P<fn_return_type>[^;{]+))?",
        )
            .unwrap();
        let ffi_lib_fn_maps = parse_function_signatures(&cleaned_ffi_lib_content, re);
        assert_eq!(fn_count, ffi_lib_fn_maps.len());

        let ffi_import_content = fs::read_to_string("src/ffi/ffi_import.rs").unwrap();
        let (fn_count, cleaned_ffi_import_content) = clean_ffi_import_content(&ffi_import_content);
        // 'cleaned_ffi_import_content' looks like:
        // ----------------------------------------
        // pub fn create_tari_vector(tag: TariTypeTag) -> *mut TariVector {
        // pub fn destroy_tari_vector(v: *mut TariVector) {
        // pub fn destroy_tari_coin_preview(p: *mut TariCoinPreview) {
        let re = Regex::new(
            r"(?m)^\s*pub\s+fn\s+(?P<fn_name>\w+)\s*\((?P<fn_args>.*)\)\s*(->\s*(?P<fn_return_type>[^;{]+))?",
        )
        .unwrap();
        let ffi_import_fn_maps = parse_function_signatures(&cleaned_ffi_import_content, re);
        assert_eq!(fn_count, ffi_import_fn_maps.len());

        let mut mismatches = Vec::new();
        for (fn_name, ffi_import_fn_map) in &ffi_import_fn_maps {
            let ffi_lib_fn_map = match ffi_lib_fn_maps.get(fn_name) {
                Some(fn_map) => fn_map,
                None => {
                    mismatches.push(format!("Function '{}' not found in ffi_lib", fn_name));
                    continue;
                },
            };
            if ffi_import_fn_map.fn_return_type != ffi_lib_fn_map.fn_return_type {
                mismatches.push(format!(
                    "Function '{}' return type mismatch:\n import: '{}'\n lib:    '{}'\n",
                    fn_name, ffi_import_fn_map.fn_return_type, ffi_lib_fn_map.fn_return_type
                ));
            }
            if ffi_import_fn_map.fn_args != ffi_lib_fn_map.fn_args {
                mismatches.push(format!(
                    "Function '{}' arguments mismatch:\n import: '{}'\n lib:    '{}'\n",
                    fn_name, ffi_import_fn_map.fn_args, ffi_lib_fn_map.fn_args
                ));
            }
        }

        if !mismatches.is_empty() {
            println!();
            for mismatch in mismatches {
                println!("{}\n", mismatch);
            }
            println!();
            panic!("Mismatched function signatures found");
        }

        // Also verify that parsing a complex function is working correctly
        let test_1 = ffi_lib_fn_maps.get("wallet_create").unwrap();
        assert_eq!(test_1.fn_return_type, "*mut TariWallet".to_string());
        assert_eq!(
            test_1.fn_args,
            "context: *mut c_void, config: *mut TariCommsConfig, database_name: *const c_char, datastore_path: *const \
             c_char, log_path: *const c_char, log_verbosity: c_int, num_rolling_log_files: c_uint, \
             size_per_log_file_bytes: c_uint, passphrase: *const c_char, seed_passphrase: *const c_char, seed_words: \
             *const TariSeedWords, network_str: *const c_char, dns_seeds_str: *const c_char, \
             dns_seed_name_servers_str: *const c_char, use_dns_sec: bool, callback_received_transaction: unsafe \
             extern \"C\" fn(context: *mut c_void, *mut TariPendingInboundTransaction), \
             callback_received_transaction_reply: unsafe extern \"C\" fn(context: *mut c_void, *mut \
             TariCompletedTransaction), callback_received_finalized_transaction: unsafe extern \"C\" fn(context: *mut \
             c_void, *mut TariCompletedTransaction), callback_transaction_broadcast: unsafe extern \"C\" fn(context: \
             *mut c_void, *mut TariCompletedTransaction), callback_transaction_mined: unsafe extern \"C\" fn(context: \
             *mut c_void, *mut TariCompletedTransaction), callback_transaction_mined_unconfirmed: unsafe extern \"C\" \
             fn(context: *mut c_void, *mut TariCompletedTransaction, u64), callback_faux_transaction_confirmed: \
             unsafe extern \"C\" fn(context: *mut c_void, *mut TariCompletedTransaction), \
             callback_faux_transaction_unconfirmed: unsafe extern \"C\" fn(context: *mut c_void, *mut \
             TariCompletedTransaction, u64), callback_transaction_send_result: unsafe extern \"C\" fn(context: *mut \
             c_void, c_ulonglong, *mut TariTransactionSendStatus), callback_transaction_cancellation: unsafe extern \
             \"C\" fn(context: *mut c_void, *mut TariCompletedTransaction, u64), callback_txo_validation_complete: \
             unsafe extern \"C\" fn(context: *mut c_void, u64, u64), callback_contacts_liveness_data_updated: unsafe \
             extern \"C\" fn(context: *mut c_void, *mut TariContactsLivenessData), callback_balance_updated: unsafe \
             extern \"C\" fn(context: *mut c_void, *mut TariBalance), callback_transaction_validation_complete: \
             unsafe extern \"C\" fn(context: *mut c_void, u64, u64), callback_saf_messages_received: unsafe extern \
             \"C\" fn(context: *mut c_void), callback_connectivity_status: unsafe extern \"C\" fn(context: *mut \
             c_void, u64), callback_wallet_scanned_height: unsafe extern \"C\" fn(context: *mut c_void, u64), \
             callback_base_node_state: unsafe extern \"C\" fn(context: *mut c_void, *mut TariBaseNodeState), \
             recovery_in_progress: *mut bool, error_out: *mut c_int"
                .to_string()
        );
    }
}
