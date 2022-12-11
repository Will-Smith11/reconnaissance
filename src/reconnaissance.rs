use ethers::providers::JsonRpcClient;
use futures::Future;
use reth_network::{
    eth_requests::EthRequestHandler, transactions::TransactionsManager, NetworkHandle,
    NetworkManager
};
use reth_transaction_pool::Pool;

use crate::{
    block_client::BlockClient,
    tx::{BasicOrdering, NonValidator}
};

/// handles all of our forwarding
pub struct Reconnaissance<P: JsonRpcClient>
{
    transaction_pool: TransactionsManager<Pool<NonValidator, BasicOrdering>>,
    network_handle:   NetworkHandle,
    network:          NetworkManager<BlockClient<P>>,
    forwarder:        EthRequestHandler<BlockClient<P>>
}

impl<P: JsonRpcClient> Reconnaissance<P> {}

impl<P: JsonRpcClient + 'static> Future for Reconnaissance<P>
{
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> std::task::Poll<Self::Output>
    {
        todo!()
    }
}
