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

use crate::automation::{commands::WalletCommand, error::ParseError};

use chrono::{DateTime, Utc};
use chrono_english::{parse_date_string, Dialect};
use core::str::SplitWhitespace;
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};
use tari_app_utilities::utilities::parse_emoji_id_or_public_key;
use tari_comms::multiaddr::Multiaddr;

use tari_core::transactions::{tari_amount::MicroTari, types::PublicKey};

#[derive(Debug)]
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
}

impl Display for ParsedArgument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use ParsedArgument::*;
        match self {
            Amount(v) => write!(f, "{}", v.to_string()),
            PublicKey(v) => write!(f, "{}", v.to_string()),
            Text(v) => write!(f, "{}", v.to_string()),
            Float(v) => write!(f, "{}", v.to_string()),
            Int(v) => write!(f, "{}", v.to_string()),
            Date(v) => write!(f, "{}", v.to_string()),
            OutputToCSVFile(v) => write!(f, "{}", v.to_string()),
            CSVFileName(v) => write!(f, "{}", v.to_string()),
            Address(v) => write!(f, "{}", v.to_string()),
            Negotiated(v) => write!(f, "{}", v.to_string()),
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
        ExportUtxos => parse_export_utxos(args)?, // todo: only show X number of utxos
        ExportSpentUtxos => parse_export_spent_utxos(args)?, // todo: only show X number of utxos
        CountUtxos => Vec::new(),
        SetBaseNode => parse_public_key_and_address(args)?,
        SetCustomBaseNode => parse_public_key_and_address(args)?,
        ClearCustomBaseNode => Vec::new(),
    };

    Ok(ParsedCommand { command, args })
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
    let now = Utc::now();
    let start_time = if start_time != "now" {
        parse_date_string(&start_time, now, Dialect::Uk).map_err(ParseError::Date)?
    } else {
        now
    };
    parsed_args.push(ParsedArgument::Date(start_time));

    // public key/emoji id
    let pubkey = args
        .next()
        .ok_or_else(|| ParseError::Empty("public key or emoji id".to_string()))?;
    let pubkey = parse_emoji_id_or_public_key(pubkey).ok_or(ParseError::PublicKey)?;
    parsed_args.push(ParsedArgument::PublicKey(pubkey));

    // transaction type
    let txn_type = args
        .next()
        .ok_or_else(|| ParseError::Empty("transaction type".to_string()))?;
    let negotiated = match txn_type {
        "negotiated" => true,
        "one_sided" => false,
        _ => {
            println!("Invalid data provided for <transaction type>, must be 'negotiated' or 'one_sided'\n");
            return Err(ParseError::Invalid(
                "Invalid data provided for <transaction type>, must be 'negotiated' or 'one_sided'".to_string(),
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

    parsed_args.push(ParsedArgument::Int(num_splits));
    Ok(parsed_args)
}

#[cfg(test)]
mod test {
    use crate::automation::{
        command_parser::{parse_command, ParsedArgument},
        error::ParseError,
    };
    use rand::rngs::OsRng;
    use std::str::FromStr;
    use tari_core::transactions::{tari_amount::MicroTari, types::PublicKey};
    use tari_crypto::keys::PublicKey as PublicKeyTrait;

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
                    "Invalid data provided for <transaction type>, must be 'negotiated' or 'one_sided'".to_string()
                ),
                _ => panic!("Expected parsing <transaction type> to return an error here"),
            },
        }
    }
}
