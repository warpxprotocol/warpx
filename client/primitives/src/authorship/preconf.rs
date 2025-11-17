use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PayloadId(pub [u8; 8]);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PreConfBlockPayload<Block: BlockT> {
    /// The payload id of the preconf block
    pub payload_id: PayloadId,
    /// The index of the `preconf block` in the block
    pub index: u64,
    /// The base of the preconf block
    pub base: Option<PreConfBlockBase<Block>>,
    /// The diff of the preconf block
    pub diff: PreConfBlockDiff<Block>,
    /// Additional metadata related to the preconf block
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize)]
pub struct PreConfBlockBase<Block: BlockT> {
    pub parent_hash: Block::Hash,
    pub block_number: <Block::Header as HeaderT>::Number,
    pub block_size_limit: usize,
    pub timestamp: u64,
}

impl<Block: BlockT> PreConfBlockBase<Block> {
    pub fn new(
        parent_hash: Block::Hash,
        block_number: <Block::Header as HeaderT>::Number,
        block_size_limit: usize,
    ) -> Self {
        Self {
            parent_hash,
            block_number,
            block_size_limit,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PreConfBlockDiff<Block: BlockT> {
    pub extrinsics: Vec<Block::Extrinsic>,
}
