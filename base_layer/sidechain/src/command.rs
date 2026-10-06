// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::io;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::types::FixedHash;
use tari_hashing::layer2::command_hasher;

use crate::serde::hex_or_bytes;

pub trait ToCommand {
    fn to_command(&self) -> Command;
}

/// A command as the sidechain commits to it in its command merkle root. Each variant's discriminant is the first
/// byte of the command's hash preimage, so it is load-bearing. 6 was the retired `EvictNode` command and must not be
/// reused.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum Command {
    LocalOnly(TransactionAtom) = 0,
    LocalPrepare(TransactionAtom) = 1,
    LocalAccept(TransactionAtom) = 2,
    AllAccept(TransactionAtom) = 3,
    SomeAccept(TransactionAtom) = 4,
    /// Carries no atom, so its hash never matches the sidechain's and no inclusion proof over it verifies.
    ForeignProposal = 5,
    EndEpoch(EndEpochAtom) = 7,
}

impl Command {
    pub fn end_epoch(&self) -> Option<&EndEpochAtom> {
        match self {
            Self::EndEpoch(end_epoch_atom) => Some(end_epoch_atom),
            _ => None,
        }
    }

    pub fn transaction(&self) -> Option<&TransactionAtom> {
        match self {
            Self::LocalOnly(atom) |
            Self::LocalPrepare(atom) |
            Self::LocalAccept(atom) |
            Self::AllAccept(atom) |
            Self::SomeAccept(atom) => Some(atom),
            Self::ForeignProposal | Self::EndEpoch(_) => None,
        }
    }

    /// The atom of a command that finalises its transaction, committing or aborting it with the atom's decision.
    pub fn finalising(&self) -> Option<&TransactionAtom> {
        match self {
            Self::LocalOnly(atom) | Self::AllAccept(atom) | Self::SomeAccept(atom) => Some(atom),
            _ => None,
        }
    }

    pub fn discriminant(&self) -> u8 {
        match self {
            Self::LocalOnly(_) => 0,
            Self::LocalPrepare(_) => 1,
            Self::LocalAccept(_) => 2,
            Self::AllAccept(_) => 3,
            Self::SomeAccept(_) => 4,
            Self::ForeignProposal => 5,
            Self::EndEpoch(_) => 7,
        }
    }

    /// The hash the sidechain commits to: its Borsh encoding of the command, which is the discriminant followed by
    /// the atom's encoding. A transaction atom is written as its bytes alone, without the length prefix it carries
    /// in this type's own Borsh encoding.
    pub fn hash(&self) -> FixedHash {
        command_hasher().chain(&CommandHashPreimage(self)).finalize().into()
    }
}

impl ToCommand for Command {
    fn to_command(&self) -> Command {
        self.clone()
    }
}

struct CommandHashPreimage<'a>(&'a Command);

impl BorshSerialize for CommandHashPreimage<'_> {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&[self.0.discriminant()])?;
        match self.0 {
            Command::LocalOnly(atom) |
            Command::LocalPrepare(atom) |
            Command::LocalAccept(atom) |
            Command::AllAccept(atom) |
            Command::SomeAccept(atom) => writer.write_all(atom.as_bytes()),
            Command::ForeignProposal => Ok(()),
            Command::EndEpoch(atom) => BorshSerialize::serialize(atom, writer),
        }
    }
}

/// A transaction command's atom as the sidechain Borsh-encodes it. The bytes are opaque except for their leading
/// fields, the transaction id followed by the decision, which every transaction atom begins with.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct TransactionAtom {
    #[serde(with = "hex_or_bytes")]
    bytes: Vec<u8>,
}

impl TransactionAtom {
    /// Wraps the sidechain's Borsh encoding of a transaction atom.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The transaction this atom sequences.
    pub fn transaction_id(&self) -> Result<FixedHash, TransactionAtomDecodeError> {
        let id = self
            .bytes
            .get(..FixedHash::byte_size())
            .ok_or(TransactionAtomDecodeError::Truncated)?;
        FixedHash::try_from(id).map_err(|_| TransactionAtomDecodeError::Truncated)
    }

    /// The decision this atom carries.
    pub fn decision(&self) -> Result<TransactionDecision, TransactionAtomDecodeError> {
        let decision = self.bytes.get(FixedHash::byte_size()..).unwrap_or_default();
        match decision {
            [] => Err(TransactionAtomDecodeError::Truncated),
            [0, ..] => Ok(TransactionDecision::Commit),
            [1, reason, ..] => Ok(TransactionDecision::Abort { reason: *reason }),
            [1] => Err(TransactionAtomDecodeError::Truncated),
            [tag, ..] => Err(TransactionAtomDecodeError::InvalidDecision(*tag)),
        }
    }
}

/// The decision a transaction atom carries. An abort's reason is the sidechain's abort reason code, which this crate
/// does not interpret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum TransactionDecision {
    Commit,
    Abort { reason: u8 },
}

impl TransactionDecision {
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::Commit)
    }

    pub fn is_abort(&self) -> bool {
        matches!(self, Self::Abort { .. })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransactionAtomDecodeError {
    #[error("Transaction atom is too short to hold a transaction id and decision")]
    Truncated,
    #[error("Invalid transaction decision tag: {0}")]
    InvalidDecision(u8),
}

/// The atom committed by an `EndEpoch` command.
///
/// It carries the base-layer boundary-block hash of the *next* epoch so that the value is ratified
/// by the committee: a validator only votes for the end-of-epoch block if `next_epoch_hash` matches
/// its own (lagged, reorg-stable) view of the next epoch's boundary block. Because this hash is part
/// of the command, it is committed in the block's command merkle root and attested by the quorum
/// that commits the block — a node can no longer unilaterally lock an epoch hash the committee never
/// agreed on (which is what wedges consensus when a base-layer reorg deeper than the confirmation
/// depth straddles the epoch boundary).
#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct EndEpochAtom {
    #[serde(with = "hex_or_bytes")]
    next_epoch_hash: FixedHash,
}

impl EndEpochAtom {
    pub fn new(next_epoch_hash: FixedHash) -> Self {
        Self { next_epoch_hash }
    }

    pub fn next_epoch_hash(&self) -> FixedHash {
        self.next_epoch_hash
    }
}

impl ToCommand for EndEpochAtom {
    fn to_command(&self) -> Command {
        Command::EndEpoch(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-ins for the sidechain's command encoding. The leading id and decision of each atom match the
    /// sidechain's; the fields after them are opaque to this crate.
    #[derive(Clone, BorshSerialize)]
    enum SidechainDecision {
        Commit,
        Abort(SidechainAbortReason),
    }

    #[derive(Clone, BorshSerialize)]
    #[allow(dead_code)]
    enum SidechainAbortReason {
        ForeignPledgeInputConflict,
        LockInputsFailed,
        LockOutputsFailed,
    }

    #[derive(Clone, BorshSerialize)]
    struct SidechainLeaderFee {
        fee: u64,
        global_exhaust_burn: u64,
    }

    #[derive(BorshSerialize)]
    struct SidechainLocalOnlyAtom {
        id: [u8; 32],
        decision: SidechainDecision,
        transaction_fee: u64,
        leader_fee: Option<SidechainLeaderFee>,
    }

    #[derive(Clone, BorshSerialize)]
    struct SidechainMultiShardAtom {
        id: [u8; 32],
        decision: SidechainDecision,
        evidence: Vec<(u32, [u8; 32])>,
        transaction_fee: u64,
        leader_fee: Option<SidechainLeaderFee>,
    }

    #[derive(BorshSerialize)]
    #[borsh(use_discriminant = true)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SidechainCommand {
        LocalOnly(SidechainLocalOnlyAtom) = 0,
        LocalPrepare(SidechainMultiShardAtom) = 1,
        LocalAccept(SidechainMultiShardAtom) = 2,
        AllAccept(SidechainMultiShardAtom) = 3,
        SomeAccept(SidechainMultiShardAtom) = 4,
    }

    fn local_only_atom(decision: SidechainDecision) -> SidechainLocalOnlyAtom {
        SidechainLocalOnlyAtom {
            id: [1; 32],
            decision,
            transaction_fee: 1000,
            leader_fee: Some(SidechainLeaderFee {
                fee: 10,
                global_exhaust_burn: 5,
            }),
        }
    }

    fn multi_shard_atom(decision: SidechainDecision) -> SidechainMultiShardAtom {
        SidechainMultiShardAtom {
            id: [2; 32],
            decision,
            evidence: vec![(0, [3; 32]), (1, [4; 32])],
            transaction_fee: 2000,
            leader_fee: None,
        }
    }

    fn atom<T: BorshSerialize>(atom: &T) -> TransactionAtom {
        TransactionAtom::from_bytes(borsh::to_vec(atom).unwrap())
    }

    fn sidechain_hash(command: &SidechainCommand) -> FixedHash {
        command_hasher().chain(command).finalize().into()
    }

    #[test]
    fn discriminants_are_stable() {
        let tx = TransactionAtom::from_bytes(vec![]);
        let cases = [
            (Command::LocalOnly(tx.clone()), 0u8),
            (Command::LocalPrepare(tx.clone()), 1),
            (Command::LocalAccept(tx.clone()), 2),
            (Command::AllAccept(tx.clone()), 3),
            (Command::SomeAccept(tx), 4),
            (Command::ForeignProposal, 5),
            (Command::EndEpoch(EndEpochAtom::new(FixedHash::zero())), 7),
        ];
        for (command, discriminant) in cases {
            let encoded = borsh::to_vec(&command).unwrap();
            assert_eq!(encoded.first(), Some(&discriminant), "{command:?}");
            assert_eq!(command.discriminant(), discriminant, "{command:?}");
        }
    }

    #[test]
    fn transaction_command_hash_matches_sidechain() {
        let local_only = local_only_atom(SidechainDecision::Commit);
        assert_eq!(
            Command::LocalOnly(atom(&local_only)).hash(),
            sidechain_hash(&SidechainCommand::LocalOnly(local_only))
        );

        for decision in [
            SidechainDecision::Commit,
            SidechainDecision::Abort(SidechainAbortReason::LockOutputsFailed),
        ] {
            let multi_shard = multi_shard_atom(decision);
            let tx = atom(&multi_shard);
            let cases = [
                (
                    Command::LocalPrepare(tx.clone()),
                    SidechainCommand::LocalPrepare(multi_shard.clone()),
                ),
                (
                    Command::LocalAccept(tx.clone()),
                    SidechainCommand::LocalAccept(multi_shard.clone()),
                ),
                (
                    Command::AllAccept(tx.clone()),
                    SidechainCommand::AllAccept(multi_shard.clone()),
                ),
                (Command::SomeAccept(tx), SidechainCommand::SomeAccept(multi_shard)),
            ];
            for (command, sidechain_command) in cases {
                assert_eq!(command.hash(), sidechain_hash(&sidechain_command), "{command:?}");
            }
        }
    }

    #[test]
    fn transaction_hash_differs_by_phase() {
        let tx = atom(&multi_shard_atom(SidechainDecision::Commit));
        assert_ne!(Command::AllAccept(tx.clone()).hash(), Command::SomeAccept(tx).hash());
    }

    #[test]
    fn end_epoch_hash_is_its_borsh_encoding() {
        let command = Command::EndEpoch(EndEpochAtom::new(FixedHash::from([9; 32])));
        let expected: FixedHash = command_hasher().chain(&command).finalize().into();
        assert_eq!(command.hash(), expected);
    }

    #[test]
    fn it_decodes_the_transaction_id_and_decision() {
        let commit = atom(&local_only_atom(SidechainDecision::Commit));
        assert_eq!(commit.transaction_id().unwrap(), FixedHash::from([1; 32]));
        assert_eq!(commit.decision().unwrap(), TransactionDecision::Commit);

        let abort = atom(&multi_shard_atom(SidechainDecision::Abort(
            SidechainAbortReason::LockOutputsFailed,
        )));
        assert_eq!(abort.transaction_id().unwrap(), FixedHash::from([2; 32]));
        assert_eq!(abort.decision().unwrap(), TransactionDecision::Abort { reason: 2 });
    }

    #[test]
    fn it_rejects_malformed_atoms() {
        let short = TransactionAtom::from_bytes(vec![1; 31]);
        assert_eq!(short.transaction_id(), Err(TransactionAtomDecodeError::Truncated));
        assert_eq!(short.decision(), Err(TransactionAtomDecodeError::Truncated));

        let no_reason = TransactionAtom::from_bytes([[1; 32].as_slice(), &[1]].concat());
        assert_eq!(no_reason.decision(), Err(TransactionAtomDecodeError::Truncated));

        let bad_tag = TransactionAtom::from_bytes([[1; 32].as_slice(), &[2]].concat());
        assert_eq!(bad_tag.decision(), Err(TransactionAtomDecodeError::InvalidDecision(2)));
    }

    #[test]
    fn only_local_only_all_accept_and_some_accept_finalise() {
        let tx = atom(&multi_shard_atom(SidechainDecision::Commit));
        let cases = [
            (Command::LocalOnly(tx.clone()), true),
            (Command::LocalPrepare(tx.clone()), false),
            (Command::LocalAccept(tx.clone()), false),
            (Command::AllAccept(tx.clone()), true),
            (Command::SomeAccept(tx), true),
            (Command::ForeignProposal, false),
            (Command::EndEpoch(EndEpochAtom::new(FixedHash::zero())), false),
        ];
        for (command, finalises) in cases {
            assert_eq!(command.finalising().is_some(), finalises, "{command:?}");
        }
    }

    #[test]
    fn it_round_trips() {
        let command = Command::SomeAccept(atom(&multi_shard_atom(SidechainDecision::Commit)));
        let decoded = Command::try_from_slice(&borsh::to_vec(&command).unwrap()).unwrap();
        assert_eq!(decoded, command);
        assert_eq!(decoded.hash(), command.hash());

        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
    }
}
