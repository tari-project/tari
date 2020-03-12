use diesel::{result::Error as DieselError, ConnectionError};
use serde_json::error::Error as SerdeJsonError;
use snafu::{ensure, Backtrace, ErrorCompat, ResultExt, Snafu};
use std::error;
use tari_core::chain_storage::{ChainStorageError, DbKey, MmrTree};
use tari_crypto::tari_utilities::{hex::HexError, ByteArrayError};
use tari_mmr::error::MerkleMountainRangeError;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub enum PostgresChainStorageError {
    #[snafu(display("Could not update {} with key `{}`:{}", entity, key, source))]
    UpdateError {
        key: String,
        entity: String,
        source: DieselError,
    },
    #[snafu(display("Could not fetch {} with key `{}`:{}", entity, key, source))]
    FetchError {
        key: String,
        entity: String,
        source: DieselError,
    },
    #[snafu(display("Could not insert {} with key `{}`:{}", entity, key, source))]
    InsertError {
        key: String,
        entity: String,
        source: DieselError,
    },
    #[snafu(display("Could not delete {} with key `{}`:{}", entity, key, source))]
    DeleteError {
        key: String,
        entity: String,
        source: DieselError,
    },
    #[snafu(display("Could not execute query{}:{}", query, source))]
    QueryError { query: String, source: DieselError },
    #[snafu(display("Could not fetch MMR checkpoint `{}`:{}", mmr_tree, source))]
    MmrFetchError { mmr_tree: MmrTree, source: DieselError },
    #[snafu(display("Could not {} MMR checkpoint `{}`:{}", action, mmr_tree, source))]
    MmrSaveError {
        mmr_tree: MmrTree,
        action: String,
        source: DieselError,
    },
    #[snafu(display("Hashed {}:{} did not match expected hash:{}", entity, actual_hash, expected_hash))]
    HashesDontMatchError {
        expected_hash: String,
        actual_hash: String,
        entity: String,
    },
    #[snafu(context(false))]
    ChainStorageError { source: ChainStorageError },
    #[snafu(context(false))]
    DieselError { source: DieselError },
    #[snafu(context(false))]
    HexError { source: HexError },
    #[snafu(context(false))]
    ConnectionError { source: ConnectionError },
    #[snafu(context(false))]
    SerializationError { source: SerdeJsonError },
    #[snafu(context(false))]
    ByteArrayError { source: ByteArrayError },
    #[snafu(context(false))]
    MerkleMountainRangeError { source: MerkleMountainRangeError },
}

impl From<PostgresChainStorageError> for ChainStorageError {
    fn from(e: PostgresChainStorageError) -> Self {
        // TODO: Flesh this out better
        ChainStorageError::AccessError(format!("Postgres error:{}", e))
    }
}
