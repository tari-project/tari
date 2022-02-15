// Copyright 2020. The Tari Project
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

use core::str::SplitWhitespace;
use std::{
    fmt::{Display, Formatter},
    iter::Peekable,
    str::FromStr,
};

use chrono::{DateTime, Utc};
use tari_app_utilities::utilities::{parse_emoji_id_or_public_key, parse_hash};
use tari_common_types::types::PublicKey;
use tari_comms::multiaddr::Multiaddr;
use tari_core::transactions::tari_amount::MicroTari;
use tari_crypto::tari_utilities::hex::Hex;

use crate::automation::{commands::WalletCommand, error::ParseError};

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub command: WalletCommand,
    pub args: Vec<ParsedArgument>,
}

impl Display for ParsedCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use WalletCommand::*;
        let command = match self.command {
            GetBalance => "get-balance",
            SendTari => "send-tari",
            SendOneSided => "send-one-sided",
            MakeItRain => "make-it-rain",
            CoinSplit => "coin-split",
            DiscoverPeer => "discover-peer",
            Whois => "whois",
            ExportUtxos => "export-utxos",
            ExportSpentUtxos => "export-spent-utxos",
            CountUtxos => "count-utxos",
            SetBaseNode => "set-base-node",
            SetCustomBaseNode => "set-custom-base-node",
            ClearCustomBaseNode => "clear-custom-base-node",
            InitShaAtomicSwap => "init-sha-atomic-swap",
            FinaliseShaAtomicSwap => "finalise-sha-atomic-swap",
            ClaimShaAtomicSwapRefund => "claim-sha-atomic-swap-refund",
            RegisterAsset => "register-asset",
            MintTokens => "mint-tokens",
            CreateInitialCheckpoint => "create-initial-checkpoint",
            CreateCommitteeDefinition => "create-committee-definition",
        };

        let args = self
            .args
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<String>>()
            .join(" ");

        write!(f, "{} {}", command, args)
    }
}

#[derive(Debug, Clone)]
pub enum ParsedArgument {
    Amount(MicroTari),
    PublicKey(PublicKey),
    Text(String),
    Float(f64),
    Int(u64),
    Date(DateTime<Utc>),
    OutputToCSVFile(String),
    CSVFileName(String),
    Address(Multiaddr),
    Negotiated(bool),
    Hash(Vec<u8>),
}

impl Display for ParsedArgument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use ParsedArgument::*;
        match self {
            Amount(v) => write!(f, "{}", v),
            PublicKey(v) => write!(f, "{}", v),
            Text(v) => write!(f, "{}", v),
            Float(v) => write!(f, "{}", v),
            Int(v) => write!(f, "{}", v),
            Date(v) => write!(f, "{}", v),
            OutputToCSVFile(v) => write!(f, "{}", v),
            CSVFileName(v) => write!(f, "{}", v),
            Address(v) => write!(f, "{}", v),
            Negotiated(v) => write!(f, "{}", v),
            Hash(v) => write!(f, "{}", v.to_hex()),
        }
    }
}

pub fn parse_command(command: &str) -> Result<ParsedCommand, ParseError> {
    let mut args = command.split_whitespace();
    let command_str = args.next().ok_or_else(|| ParseError::Empty("command".to_string()))?;

    let command =
        WalletCommand::from_str(command_str).map_err(|_| ParseError::WalletCommand(command_str.to_string()))?;

    use WalletCommand::*;
    let args = match command {
        GetBalance => Vec::new(),
        SendTari => parse_send_tari(args)?,
        SendOneSided => parse_send_tari(args)?,
        MakeItRain => parse_make_it_rain(args)?,
        CoinSplit => parse_coin_split(args)?,
        DiscoverPeer => parse_public_key(args)?,
        Whois => parse_whois(args)?,
        ExportUtxos => parse_export_utxos(args)?,
        ExportSpentUtxos => parse_export_spent_utxos(args)?,
        CountUtxos => Vec::new(),
        SetBaseNode => parse_public_key_and_address(args)?,
        SetCustomBaseNode => parse_public_key_and_address(args)?,
        ClearCustomBaseNode => Vec::new(),
        InitShaAtomicSwap => parse_init_sha_atomic_swap(args)?,
        FinaliseShaAtomicSwap => parse_finalise_sha_atomic_swap(args)?,
        ClaimShaAtomicSwapRefund => parse_claim_htlc_refund_refund(args)?,
        RegisterAsset => parser_builder(args).text().build()?,
        // mint-tokens pub_key nft_id1 nft_id2
        MintTokens => parser_builder(args).pub_key().text_array().build()?,
        CreateInitialCheckpoint => parser_builder(args).pub_key().text().build()?,
        CreateCommitteeDefinition => parser_builder(args).pub_key().pub_key_array().build()?,
    };

    Ok(ParsedCommand { command, args })
}

struct ArgParser<'a> {
    args: Peekable<SplitWhitespace<'a>>,
    result: Vec<Result<ParsedArgument, ParseError>>,
}

impl<'a> ArgParser<'a> {
    fn new(args: SplitWhitespace<'a>) -> Self {
        Self {
            args: args.peekable(),
            result: vec![],
        }
    }

    fn text(mut self) -> Self {
        let text_result = self
            .args
            .next()
            .map(|t| ParsedArgument::Text(t.to_string()))
            .ok_or_else(|| ParseError::Empty("text".to_string()));
        self.result.push(text_result);
        self
    }

    fn text_array(self) -> Self {
        let mut me = self;
        while me.args.peek().is_some() {
            me = me.text();
        }

        me
    }

    fn pub_key(mut self) -> Self {
        // public key/emoji id
        let pubkey = self
            .args
            .next()
            .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()));
        let result = pubkey.and_then(
            |pb| match parse_emoji_id_or_public_key(pb).ok_or(ParseError::PublicKey) {
                Ok(pk) => Ok(ParsedArgument::PublicKey(pk)),
                Err(err) => Err(err),
            },
        );
        self.result.push(result);
        self
    }

    fn pub_key_array(self) -> Self {
        let mut me = self;
        while me.args.peek().is_some() {
            me = me.pub_key();
        }

        me
    }

    fn build(self) -> Result<Vec<ParsedArgument>, ParseError> {
        let mut result = Vec::with_capacity(self.result.len());
        for r in self.result {
            result.push(r?);
        }
        Ok(result)
    }
}

fn parser_builder(args: SplitWhitespace) -> ArgParser {
    ArgParser::new(args)
}

fn parse_whois(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    // public key/emoji id
    let pubkey = args
        .next()
        .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()))?;
    let pubkey = parse_emoji_id_or_public_key(pubkey).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pubkey));

    Ok(parsed_args)
}

fn parse_public_key(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    // public key/emoji id
    let pubkey = args
        .next()
        .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()))?;
    let pubkey = parse_emoji_id_or_public_key(pubkey).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pubkey));

    Ok(parsed_args)
}

fn parse_public_key_and_address(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    // public key/emoji id
    let pubkey = args
        .next()
        .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()))?;
    let pubkey = parse_emoji_id_or_public_key(pubkey).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pubkey));

    // address
    let address = args
        .next()
        .ok_or_else(|| ParseError::Empty("net address".to_string()))?;
    let address = address.parse::<Multiaddr>().map_err(|_| ParseError::Address)?;
    parsed_args.push(ParsedArgument::Address(address));

    Ok(parsed_args)
}

fn parse_init_sha_atomic_swap(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    // amount
    let amount = args.next().ok_or_else(|| ParseError::Empty("amount".to_string()))?;
    let amount = MicroTari::from_str(amount)?;
    parsed_args.push(ParsedArgument::Amount(amount));

    // public key/emoji id
    let pubkey = args
        .next()
        .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()))?;
    let pubkey = parse_emoji_id_or_public_key(pubkey).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pubkey));
    // message
    let message = args.collect::<Vec<&str>>().join(" ");
    parsed_args.push(ParsedArgument::Text(message));

    Ok(parsed_args)
}

fn parse_finalise_sha_atomic_swap(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();
    // hash
    let hash = args
        .next()
        .ok_or_else(|| ParseError::Empty("Output hash".to_string()))?;
    let hash = parse_hash(hash).ok_or(ParseError::Hash)?;
    parsed_args.push(ParsedArgument::Hash(hash));

    // public key
    let pre_image = args.next().ok_or_else(|| ParseError::Empty("public key".to_string()))?;
    let pre_image = parse_emoji_id_or_public_key(pre_image).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pre_image));

    Ok(parsed_args)
}

fn parse_claim_htlc_refund_refund(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();
    // hash
    let hash = args
        .next()
        .ok_or_else(|| ParseError::Empty("Output hash".to_string()))?;
    let hash = parse_hash(hash).ok_or(ParseError::Hash)?;
    parsed_args.push(ParsedArgument::Hash(hash));

    Ok(parsed_args)
}

fn parse_make_it_rain(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    // txs per second
    let txps = args.next().ok_or_else(|| ParseError::Empty("Txs/s".to_string()))?;
    let txps = txps.parse::<f64>().map_err(ParseError::Float)?;
    if txps > 25.0 {
        println!("Maximum transaction rate is 25/sec");
        return Err(ParseError::Invalid("Maximum transaction rate is 25/sec".to_string()));
    }
    parsed_args.push(ParsedArgument::Float(txps));

    // duration
    let duration = args.next().ok_or_else(|| ParseError::Empty("duration".to_string()))?;
    let duration = duration.parse::<u64>().map_err(ParseError::Int)?;
    parsed_args.push(ParsedArgument::Int(duration));

    if (txps * duration as f64) < 1.0 {
        println!("Invalid data provided for [number of Txs/s] * [test duration (s)], must be >= 1\n");
        return Err(ParseError::Invalid(
            "Invalid data provided for [number of Txs/s] * [test duration (s)], must be >= 1".to_string(),
        ));
    }

    // start amount
    let start_amount = args
        .next()
        .ok_or_else(|| ParseError::Empty("start amount".to_string()))?;
    let start_amount = MicroTari::from_str(start_amount)?;
    parsed_args.push(ParsedArgument::Amount(start_amount));

    // increment amount
    let inc_amount = args
        .next()
        .ok_or_else(|| ParseError::Empty("increment amount".to_string()))?;
    let inc_amount = MicroTari::from_str(inc_amount)?;
    parsed_args.push(ParsedArgument::Amount(inc_amount));

    // start time utc or 'now'
    let start_time = args.next().ok_or_else(|| ParseError::Empty("start time".to_string()))?;
    let start_time = if start_time != "now" {
        DateTime::parse_from_rfc3339(start_time)?.with_timezone(&Utc)
    } else {
        Utc::now()
    };
    parsed_args.push(ParsedArgument::Date(start_time));

    // public key/emoji id
    let pubkey = args
        .next()
        .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()))?;
    let pubkey = parse_emoji_id_or_public_key(pubkey).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pubkey));

    // transaction type
    let txn_type = args.next();
    let negotiated = match txn_type {
        Some("negotiated") | Some("interactive") => true,
        Some("one_sided") | Some("one-sided") | Some("onesided") => false,
        _ => {
            println!("Invalid data provided for <transaction type>, must be 'interactive' or 'one-sided'\n");
            return Err(ParseError::Invalid(
                "Invalid data provided for <transaction type>, must be 'interactive' or 'one-sided'".to_string(),
            ));
        },
    };
    parsed_args.push(ParsedArgument::Negotiated(negotiated));

    // message
    let message = args.collect::<Vec<&str>>().join(" ");
    parsed_args.push(ParsedArgument::Text(message));

    Ok(parsed_args)
}

fn parse_send_tari(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    // amount
    let amount = args.next().ok_or_else(|| ParseError::Empty("amount".to_string()))?;
    let amount = MicroTari::from_str(amount)?;
    parsed_args.push(ParsedArgument::Amount(amount));

    // public key/emoji id
    let pubkey = args
        .next()
        .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()))?;
    let pubkey = parse_emoji_id_or_public_key(pubkey).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pubkey));

    // message
    let message = args.collect::<Vec<&str>>().join(" ");
    parsed_args.push(ParsedArgument::Text(message));

    Ok(parsed_args)
}

fn parse_export_utxos(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    if let Some(v) = args.next() {
        if v == "--csv-file" {
            let file_name = args.next().ok_or_else(|| {
                ParseError::Empty(
                    "file name\n  Usage:\n    export-utxos\n    export-utxos --csv-file <file name>".to_string(),
                )
            })?;
            parsed_args.push(ParsedArgument::OutputToCSVFile("--csv-file".to_string()));
            parsed_args.push(ParsedArgument::CSVFileName(file_name.to_string()));
        } else {
            return Err(ParseError::Empty(
                "'--csv-file' qualifier\n  Usage:\n    export-utxos\n    export-utxos --csv-file <file name>"
                    .to_string(),
            ));
        }
    };

    Ok(parsed_args)
}

fn parse_export_spent_utxos(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = Vec::new();

    if let Some(v) = args.next() {
        if v == "--csv-file" {
            let file_name = args.next().ok_or_else(|| {
                ParseError::Empty(
                    "file name\n  Usage:\n    export-spent-utxos\n    export-spent-utxos --csv-file <file name>"
                        .to_string(),
                )
            })?;
            parsed_args.push(ParsedArgument::OutputToCSVFile("--csv-file".to_string()));
            parsed_args.push(ParsedArgument::CSVFileName(file_name.to_string()));
        } else {
            return Err(ParseError::Empty(
                "'--csv-file' qualifier\n  Usage:\n    export-spent-utxos\n    export-spent-utxos --csv-file <file \
                 name>"
                    .to_string(),
            ));
        }
    };

    Ok(parsed_args)
}

fn parse_coin_split(mut args: SplitWhitespace) -> Result<Vec<ParsedArgument>, ParseError> {
    let mut parsed_args = vec![];

    let amount_per_split = args
        .next()
        .ok_or_else(|| ParseError::Empty("amount_per_split".to_string()))?;
    let amount_per_split = MicroTari::from_str(amount_per_split)?;
    parsed_args.push(ParsedArgument::Amount(amount_per_split));
    let num_splits = args
        .next()
        .ok_or_else(|| ParseError::Empty("split_count".to_string()))?;
    let num_splits = num_splits.parse::<u64>()?;
    let fee_per_gram = args.next().unwrap_or("5");
    parsed_args.push(ParsedArgument::Int(num_splits));
    let fee_per_gram = MicroTari::from_str(fee_per_gram)?;
    parsed_args.push(ParsedArgument::Amount(fee_per_gram));
    Ok(parsed_args)
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use rand::rngs::OsRng;
    use tari_common_types::types::PublicKey;
    use tari_core::transactions::tari_amount::MicroTari;
    use tari_crypto::keys::PublicKey as PublicKeyTrait;

    use crate::automation::{
        command_parser::{parse_command, ParsedArgument},
        error::ParseError,
    };

    #[test]
    fn test_parse_command() {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        let command_str = "";
        let parsed = parse_command(command_str);
        assert!(parsed.is_err());

        let command_str = "send-tari asdf";
        let parsed = parse_command(command_str);
        assert!(parsed.is_err());

        let command_str = "send-tari 999T";
        let parsed = parse_command(command_str);
        assert!(parsed.is_err());

        let command_str = "send-tari 999T asdf";
        let parsed = parse_command(command_str);
        assert!(parsed.is_err());

        let command_str = format!("send-tari 999T {} msg text", public_key);
        let parsed = parse_command(&command_str).unwrap();

        if let ParsedArgument::Amount(amount) = parsed.args[0].clone() {
            assert_eq!(amount, MicroTari::from_str("999T").unwrap());
        } else {
            panic!("Parsed MicroTari amount not the same as provided.");
        }
        if let ParsedArgument::PublicKey(pk) = parsed.args[1].clone() {
            assert_eq!(pk, public_key);
        } else {
            panic!("Parsed public key is not the same as provided.");
        }
        if let ParsedArgument::Text(msg) = parsed.args[2].clone() {
            assert_eq!(msg, "msg text");
        } else {
            panic!("Parsed message is not the same as provided.");
        }

        let command_str = format!("send-tari 999ut {}", public_key);
        let parsed = parse_command(&command_str).unwrap();

        if let ParsedArgument::Amount(amount) = parsed.args[0].clone() {
            assert_eq!(amount, MicroTari::from_str("999ut").unwrap());
        } else {
            panic!("Parsed MicroTari amount not the same as provided.");
        }

        let command_str = format!("send-tari 999 {}", public_key);
        let parsed = parse_command(&command_str).unwrap();

        if let ParsedArgument::Amount(amount) = parsed.args[0].clone() {
            assert_eq!(amount, MicroTari::from_str("999").unwrap());
        } else {
            panic!("Parsed MicroTari amount not the same as provided.");
        }

        let command_str = format!("discover-peer {}", public_key);
        let parsed = parse_command(&command_str).unwrap();

        if let ParsedArgument::PublicKey(pk) = parsed.args[0].clone() {
            assert_eq!(pk, public_key);
        } else {
            panic!("Parsed public key is not the same as provided.");
        }

        let command_str = "export-utxos --csv-file utxo_list.csv".to_string();
        let parsed = parse_command(&command_str).unwrap();

        if let ParsedArgument::CSVFileName(file) = parsed.args[1].clone() {
            assert_eq!(file, "utxo_list.csv".to_string());
        } else {
            panic!("Parsed csv file name is not the same as provided.");
        }

        let transaction_type = "negotiated";
        let message = "Testing the network!";
        let command_str = format!(
            "make-it-rain 20 225 9000 0 now {} {} {}",
            public_key, transaction_type, message
        );
        let parsed = parse_command(&command_str).unwrap();

        if let ParsedArgument::PublicKey(pk) = parsed.args[5].clone() {
            assert_eq!(pk, public_key);
        } else {
            panic!("Parsed public key is not the same as provided.");
        }
        if let ParsedArgument::Negotiated(negotiated) = parsed.args[6].clone() {
            assert!(negotiated);
        } else {
            panic!("Parsed <transaction type> is not the same as provided.");
        }
        if let ParsedArgument::Text(msg) = parsed.args[7].clone() {
            assert_eq!(message, msg);
        } else {
            panic!("Parsed message is not the same as provided.");
        }

        let transaction_type = "one_sided";
        let command_str = format!(
            "make-it-rain 20 225 9000 0 now {} {} {}",
            public_key, transaction_type, message
        );
        let parsed = parse_command(&command_str).unwrap();

        if let ParsedArgument::Negotiated(negotiated) = parsed.args[6].clone() {
            assert!(!negotiated);
        } else {
            panic!("Parsed <transaction type> is not the same as provided.");
        }

        let transaction_type = "what_ever";
        let command_str = format!(
            "make-it-rain 20 225 9000 0 now {} {} {}",
            public_key, transaction_type, message
        );
        match parse_command(&command_str) {
            Ok(_) => panic!("<transaction type> argument '{}' not allowed", transaction_type),
            Err(e) => match e {
                ParseError::Invalid(e) => assert_eq!(
                    e,
                    "Invalid data provided for <transaction type>, must be 'interactive' or 'one-sided'".to_string()
                ),
                _ => panic!("Expected parsing <transaction type> to return an error here"),
            },
        }
    }
}
