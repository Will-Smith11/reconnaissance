use std::sync::{atomic::AtomicUsize, Arc};

use ethers::{
    providers::{JsonRpcClient, Middleware, Provider},
    types::{BlockId, BlockNumber, H256 as EH256}
};
use reth_eth_wire::BlockBody;
use reth_interfaces::p2p::error::RequestResult;
use reth_primitives::{Header, H256};
use reth_provider::{BlockHashProvider, BlockProvider, ChainInfo, HeaderProvider};
use reth_tracing::tracing::debug;
use tokio::{
    join,
    runtime::Handle,
    sync::{mpsc::UnboundedSender, oneshot}
};
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
        let max = inner.len() - 1;
        Cycler { inner, ptr, max }
    }

    pub fn get(&self) -> &T
    {
        let ptr = if self.ptr.load(std::sync::atomic::Ordering::SeqCst) >= self.max
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

/// very jank because we are trying todo async over sync  
pub struct BlockClient<P: JsonRpcClient>
{
    // different clients to delegate calls to
    provider: Cycler<Arc<Provider<P>>>,
    fetcher:  UnboundedSender<FetcherReq>
}

#[derive(Debug)]
pub enum FetcherReq
{
    BlockBodyReq((oneshot::Sender<RequestResult<Vec<BlockBody>>>, Vec<H256>))
}

impl<P: JsonRpcClient> BlockClient<P>
{
    pub fn new(inner: Cycler<Arc<Provider<P>>>, fetcher: UnboundedSender<FetcherReq>) -> Self
    {
        Self { provider: inner, fetcher }
    }

    async fn get_block(
        &self,
        id: BlockId
    ) -> reth_interfaces::Result<Option<ethers::types::Block<EH256>>>
    {
        self.provider.get().get_block(id).await.map_err(|_| {
            reth_interfaces::Error::Provider(reth_provider::Error::BlockNumber { block_number: 0 })
        })
    }

    fn build_header(&self, block: ethers::types::Block<EH256>) -> reth_interfaces::Result<Header>
    {
        Ok(Header {
            parent_hash:       block.parent_hash.0.into(),
            ommers_hash:       block.uncles_hash.0.into(),
            beneficiary:       block.author.unwrap().0.into(),
            state_root:        block.state_root.0.into(),
            transactions_root: block.transactions_root.0.into(),
            receipts_root:     block.receipts_root.0.into(),
            logs_bloom:        block.logs_bloom.unwrap().0.into(),
            difficulty:        block.difficulty.into(),
            number:            block.number.unwrap().as_u64(),
            gas_limit:         block.gas_limit.as_u64(),
            gas_used:          block.gas_used.as_u64(),
            timestamp:         block.timestamp.as_u64(),
            mix_hash:          block.mix_hash.unwrap().0.into(),
            nonce:             block.nonce.unwrap().to_low_u64_be(),
            base_fee_per_gas:  block.base_fee_per_gas.map(|i| i.as_u64()),
            extra_data:        reth_primitives::Bytes(block.extra_data.0)
        })
    }
}

impl<P: JsonRpcClient> BlockHashProvider for BlockClient<P>
{
    fn block_hash(
        &self,
        number: reth_primitives::U256
    ) -> reth_interfaces::Result<Option<reth_primitives::H256>>
    {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.provider
                    .get()
                    .get_block(BlockId::Number(BlockNumber::Number(number.to::<u64>().into())))
                    .await
            })
        })
        .map(|block| block.map(|block_inner| block_inner.hash.unwrap().0.into()))
        .map_err(|_| {
            reth_interfaces::Error::Provider(reth_provider::Error::BlockNumber {
                block_number: number.to::<u64>()
            })
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
                debug!("chain info call success");
                Ok(ChainInfo {
                    best_hash:      latest_block.hash.unwrap().0.into(),
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
                    reth_interfaces::Error::Provider(reth_provider::Error::BlockNumber {
                        block_number: 0
                    })
                })?;
                let (send, recv) = tokio::sync::oneshot::channel();
                self.fetcher
                    .send(FetcherReq::BlockBodyReq((send, vec![block.hash.unwrap().0.into()])))
                    .unwrap();

                let mut body = recv.await.unwrap().map_err(|_| {
                    reth_interfaces::Error::Provider(reth_provider::Error::BlockNumber {
                        block_number: 0
                    })
                })?;
                if body.is_empty()
                {
                    return Err(reth_interfaces::Error::Provider(
                        reth_provider::Error::BlockNumber { block_number: 0 }
                    ))
                }
                let res = body.remove(0);

                let header = self.build_header(block)?;
                let block =
                    reth_primitives::Block { header, body: res.transactions, ommers: res.ommers };
                debug!("block call success");

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
            Handle::current().block_on(async {
                self.provider
                    .get()
                    .get_block(BlockId::Hash(hash.0.into()))
                    .await
            })
        })
        .map(|block| block.map(|block_inner| block_inner.number.unwrap().as_u64()))
        .map_err(|_| {
            reth_interfaces::Error::Provider(reth_provider::Error::BlockHash { block_hash: hash })
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
                    self.get_block(BlockId::Hash(block_hash.0.into()))
                        .await?
                        .ok_or_else(|| {
                            reth_interfaces::Error::Provider(reth_provider::Error::BlockNumber {
                                block_number: 0
                            })
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
                            reth_interfaces::Error::Provider(reth_provider::Error::BlockNumber {
                                block_number: 0
                            })
                        })?
                )
                .map(Some)
            })
        })
    }

    fn header_td(
        &self,
        hash: &reth_primitives::BlockHash
    ) -> reth_interfaces::Result<Option<reth_primitives::U256>>
    {
        tokio::task::block_in_place(move || {
            Handle::current().block_on(async {
                let block = self
                    .get_block(BlockId::Hash(hash.0.into()))
                    .await?
                    .ok_or_else(|| {
                        reth_interfaces::Error::Provider(reth_provider::Error::BlockHash {
                            block_hash: *hash
                        })
                    })?;
                Ok(Some(block.difficulty.into()))
            })
        })
    }
}
