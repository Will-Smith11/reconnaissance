use std::sync::{atomic::AtomicUsize, Arc};

use ethers::{
    providers::{JsonRpcClient, Middleware, Provider},
    types::{BlockId, BlockNumber}
};
use reth_provider::{BlockProvider, ChainInfo};
use tokio::{join, runtime::Handle};

/// takes any value and cycles through them
pub struct Cycler<T>
{
    inner: Vec<T>,
    ptr:   AtomicUsize,
    max:   usize
}
impl<T> Cycler<T>
{
    pub fn get(&self) -> Option<&T>
    {
        let ptr = if self.ptr.load(std::sync::atomic::Ordering::SeqCst) == self.max
        {
            self.ptr.store(0, std::sync::atomic::Ordering::SeqCst);
            0
        }
        else
        {
            self.ptr.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        };
        self.inner.get(ptr)
    }
}
/// bit jank because the blockProvider api isn't async
pub struct BlockClient<P: JsonRpcClient>
{
    // different clients to delgate calls to
    inner: Cycler<Arc<Provider<P>>>
}

impl<P: JsonRpcClient> BlockProvider for BlockClient<P>
{
    fn chain_info(&self) -> reth_interfaces::Result<reth_provider::ChainInfo>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                let latest = self
                    .inner
                    .get()
                    .unwrap()
                    .get_block(BlockId::Number(BlockNumber::Latest));
                let finalized = self
                    .inner
                    .get()
                    .unwrap()
                    .get_block(BlockId::Number(BlockNumber::Finalized));
                let safe = self
                    .inner
                    .get()
                    .unwrap()
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
                let err = || reth_interfaces::Error::Database(reth_interfaces::db::Error::Read(69));
                let block = self
                    .inner
                    .get()
                    .unwrap()
                    .get_block(id)
                    .await
                    .map_err(|_| {
                        reth_interfaces::Error::Database(reth_interfaces::db::Error::Read(69))
                    })?
                    .ok_or_else(err)?;

                Ok(Some(reth_primitives::Block {
                    header: todo!(),
                    body:   todo!(),
                    ommers: todo!()
                }))
            })
        })
    }

    fn block_number(
        &self,
        hash: reth_primitives::H256
    ) -> reth_interfaces::Result<Option<reth_primitives::BlockNumber>>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.inner
                    .get()
                    .unwrap()
                    .get_block(BlockId::Hash(hash))
                    .await
            })
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
                    .unwrap()
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
