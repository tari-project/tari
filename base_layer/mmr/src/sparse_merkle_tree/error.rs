use thiserror::Error;

use crate::sparse_merkle_tree::NodeKey;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SMTError {
    #[error("Source data too short ({0})")]
    ArrayTooShort(usize),
    #[error("Invalid branch: {0}")]
    InvalidBranch(String),
    #[error("Tried to traverse to an invalid child key ({child_key}) at height {height} from parent {parent_key}.")]
    InvalidChildKey {
        height: usize,
        parent_key: NodeKey,
        child_key: NodeKey,
    },
    #[error("find_terminal returned a branch node")]
    InvalidTerminalNode,
    #[error("Changing a branch node would lead to loss of data")]
    CannotMutateBranchNode,
    #[error("Expected an empty node")]
    ExpectedEmptyNode,
    #[error("A node is not of the expected type")]
    UnexpectedNodeType,
    #[error("The key is not valid: {0}")]
    IllegalKey(String),
}
