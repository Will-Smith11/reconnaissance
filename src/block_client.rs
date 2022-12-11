use std::sync::{atomic::AtomicUsize, Arc};

use ethers::{
    prelude::k256::sha2::digest::typenum::private::IsEqualPrivate,
    providers::{JsonRpcClient, Middleware, Provider},
    types::{BlockId, BlockNumber}
};
use futures::future::join_all;
use reth_interfaces::p2p::{bodies::client::BodiesClient, headers::client::HeadersClient};
use reth_network::FetchClient;
use reth_primitives::{Header, TxHash, H256};
use reth_provider::{BlockProvider, ChainInfo, HeaderProvider};
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
    pub fn new(inner: Vec<T>) -> Self
    {
        let ptr = AtomicUsize::new(0);
        let max = inner.len();
        Cycler { inner, ptr, max }
    }

    pub fn get(&self) -> &T
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
        self.inner.get(ptr).unwrap()
    }
}

/// very jank because we are trying todo async over sink  
pub struct BlockClient<P: JsonRpcClient>
{
    // different clients to delegate calls to
    provider: Cycler<Arc<Provider<P>>>,
    fetcher:  FetchClient
}

impl<P: JsonRpcClient> BlockClient<P>
{
    pub fn new(inner: Cycler<Arc<Provider<P>>>, fetcher: FetchClient) -> Self
    {
        Self { provider: inner, fetcher }
    }

    async fn get_block(
        &self,
        id: BlockId
    ) -> reth_interfaces::Result<Option<ethers::types::Block<TxHash>>>
    {
        self.provider.get().get_block(id).await.map_err(|_| {
            reth_interfaces::Error::Provider(reth_provider::Error::BlockNumberNotExists {
                block_number: 0
            })
        })
    }

    fn build_header(&self, block: ethers::types::Block<TxHash>) -> reth_interfaces::Result<Header>
    {
        Ok(Header {
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
            base_fee_per_gas:  block.base_fee_per_gas.map(|i| i.as_u64()),
            extra_data:        block.extra_data.0
        })
    }
}

impl<P: JsonRpcClient> BlockProvider for BlockClient<P>
{
    fn chain_info(&self) -> reth_interfaces::Result<reth_provider::ChainInfo>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                let latest = self
                    .provider
                    .get()
                    .get_block(BlockId::Number(BlockNumber::Latest));
                let finalized = self
                    .provider
                    .get()
                    .get_block(BlockId::Number(BlockNumber::Finalized));
                let safe = self
                    .provider
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
                let block = self.get_block(id).await?.ok_or_else(|| {
                    reth_interfaces::Error::Provider(reth_provider::Error::BlockNumberNotExists {
                        block_number: 0
                    })
                })?;

                let body = self
                    .fetcher
                    .get_block_body(vec![block.hash.unwrap()])
                    .await
                    .map_err(|_| {
                        reth_interfaces::Error::Provider(
                            reth_provider::Error::BlockNumberNotExists { block_number: 0 }
                        )
                    })?
                    .remove(0);

                let header = self.build_header(block)?;
                let block =
                    reth_primitives::Block { header, body: body.transactions, ommers: body.ommers };

                Ok(Some(block))
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
                .block_on(async { self.provider.get().get_block(BlockId::Hash(hash)).await })
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
                self.provider
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

impl<P: JsonRpcClient> HeaderProvider for BlockClient<P>
{
    fn header(
        &self,
        block_hash: &reth_primitives::BlockHash
    ) -> reth_interfaces::Result<Option<reth_primitives::Header>>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.build_header(
                    self.get_block(BlockId::Hash(*block_hash))
                        .await?
                        .ok_or_else(|| {
                            reth_interfaces::Error::Provider(
                                reth_provider::Error::BlockNumberNotExists { block_number: 0 }
                            )
                        })?
                )
                .map(Some)
            })
        })
    }

    fn header_by_number(&self, num: u64)
        -> reth_interfaces::Result<Option<reth_primitives::Header>>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.build_header(
                    self.get_block(BlockId::Number(BlockNumber::Number(num.into())))
                        .await?
                        .ok_or_else(|| {
                            reth_interfaces::Error::Provider(
                                reth_provider::Error::BlockNumberNotExists { block_number: 0 }
                            )
                        })?
                )
                .map(Some)
            })
        })
    }
}
