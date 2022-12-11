use std::sync::{atomic::AtomicUsize, Arc};

use ethers::{
    providers::{JsonRpcClient, Middleware, Provider},
    types::{BlockId, BlockNumber}
};
use futures::future::join_all;
use reth_network::FetchClient;
use reth_primitives::Header;
use reth_provider::{BlockProvider, ChainInfo, HeaderProvider};
use tokio::{join, runtime::Handle};

/// bit jank because the blockProvider api isn't async
pub struct BlockClient
{
    // different clients to delegate calls to
    fetcher: FetchClient
}

impl BlockClient
{
    pub fn new(fetcher: FetchClient) -> Self
    {
        Self { fetcher }
    }
}

impl BlockProvider for BlockClient
{
    fn chain_info(&self) -> reth_interfaces::Result<reth_provider::ChainInfo>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                let latest = self
                    .inner
                    .get()
                    .get_block(BlockId::Number(BlockNumber::Latest));
                let finalized = self
                    .inner
                    .get()
                    .get_block(BlockId::Number(BlockNumber::Finalized));
                let safe = self
                    .inner
                    .get()
                    .get_block(BlockId::Number(BlockNumber::Safe));
                let (latest, finalized, safe) = join!(latest, finalized, safe);

                // use 69 as its most optimal lol
                let take_err =
                    |_| reth_interfaces::Error::Database(reth_interfaces::db::Error::Read(69));
                let err = || reth_interfaces::Error::Database(reth_interfaces::db::Error::Read(69));

                let latest_block = latest.map_err(take_err)?.ok_or_else(err)?;
                let safe_block = safe.map_err(take_err)?.ok_or_else(err)?;
                let finalized_block = finalized.map_err(take_err)?.ok_or_else(err)?;

                Ok(ChainInfo {
                    best_hash:      latest_block.hash.unwrap(),
                    best_number:    latest_block.number.unwrap().as_u64(),
                    last_finalized: finalized_block.number.map(|e| e.as_u64()),
                    safe_finalized: safe_block.number.map(|e| e.as_u64())
                })
            })
        })
    }

    fn block(
        &self,
        id: reth_primitives::rpc::BlockId
    ) -> reth_interfaces::Result<Option<reth_primitives::Block>>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                let block = self.inner.get().get_block(id).await.unwrap().unwrap();

                let header = Header {
                    parent_hash:       block.parent_hash,
                    ommers_hash:       block.uncles_hash,
                    beneficiary:       block.author.unwrap(),
                    state_root:        block.state_root,
                    transactions_root: block.transactions_root,
                    receipts_root:     block.receipts_root,
                    logs_bloom:        block.logs_bloom.unwrap(),
                    difficulty:        block.difficulty,
                    number:            block.number.unwrap().as_u64(),
                    gas_limit:         block.gas_limit.as_u64(),
                    gas_used:          block.gas_used.as_u64(),
                    timestamp:         block.timestamp.as_u64(),
                    mix_hash:          block.mix_hash.unwrap(),
                    nonce:             block.nonce.unwrap().to_low_u64_be(),
                    base_fee_per_gas:  block.base_fee_per_gas.map(|f| f.as_u64()),
                    extra_data:        block.extra_data.0
                };

                // self.inner.get().get_uncle(block_hash_or_number, idx)

                let futs = join_all(
                    block
                        .transactions
                        .into_iter()
                        .map(|tx| self.inner.get().get_transaction(tx))
                )
                .await
                .into_iter()
                .map(|res| {});

                Ok(Some(reth_primitives::Block { header, body: todo!(), ommers: todo!() }))
                // Ok(None)
            })
        })
    }

    fn block_number(
        &self,
        hash: reth_primitives::H256
    ) -> reth_interfaces::Result<Option<reth_primitives::BlockNumber>>
    {
        tokio::task::block_in_place(|| {
            Handle::current()
                .block_on(async { self.inner.get().get_block(BlockId::Hash(hash)).await })
        })
        .map(|block| block.map(|block_inner| block_inner.number.unwrap().as_u64()))
        .map_err(|_| {
            reth_interfaces::Error::Provider(reth_provider::Error::BlockHashNotExist {
                block_hash: hash
            })
        })
    }

    fn block_hash(
        &self,
        number: reth_primitives::U256
    ) -> reth_interfaces::Result<Option<reth_primitives::H256>>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.inner
                    .get()
                    .get_block(BlockId::Number(BlockNumber::Number(number.as_u64().into())))
                    .await
            })
        })
        .map(|block| block.map(|block_inner| block_inner.hash.unwrap()))
        .map_err(|_| {
            reth_interfaces::Error::Provider(reth_provider::Error::BlockNumberNotExists {
                block_number: number.as_u64()
            })
        })
    }
}

impl HeaderProvider for BlockClient
{
    fn header(
        &self,
        block_hash: &reth_primitives::BlockHash
    ) -> reth_interfaces::Result<Option<reth_primitives::Header>>
    {
        todo!()
    }

    fn header_by_number(&self, num: u64)
        -> reth_interfaces::Result<Option<reth_primitives::Header>>
    {
        todo!()
    }
}
