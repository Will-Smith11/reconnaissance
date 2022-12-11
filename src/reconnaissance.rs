use std::{sync::Arc, task::Poll};

use ethers::providers::JsonRpcClient;
use futures::{Future, FutureExt};
use reth_network::{
    eth_requests::EthRequestHandler, transactions::TransactionsManager, NetworkConfig,
    NetworkManager
};
use reth_transaction_pool::{Pool, PoolConfig};
use secp256k1::SecretKey;
use tokio::{sync::mpsc::unbounded_channel, task::JoinHandle};

use crate::{
    block_client::BlockClient,
    tx::{BasicOrdering, NonValidator}
};

/// handles all of our forwarding
pub struct Reconnaissance
{
    handles: Vec<JoinHandle<()>>
}

impl Reconnaissance
{
    pub async fn new<P: JsonRpcClient + 'static>(client: Arc<BlockClient<P>>) -> Self
    {
        let secret_key = SecretKey::new(&mut rand::thread_rng());
        let network_config = NetworkConfig::builder(client.clone(), secret_key).build();

        let mut network_mng = NetworkManager::new(network_config).await.unwrap();
        let network_handle = network_mng.handle().clone();
        let ordering = BasicOrdering;
        let fake_val = NonValidator;
        // comm channels
        let (eth_sender, eth_recv) = unbounded_channel();
        let (tx_sender, tx_recv) = unbounded_channel();
        // set comms
        network_mng.set_eth_request_handler(eth_sender);
        network_mng.set_transactions(tx_sender);

        let transaction_pool = Pool::new(fake_val.into(), ordering.into(), PoolConfig::default());
        let transaction_msg = TransactionsManager::new(network_handle, transaction_pool, tx_recv);
        let peer_handle = network_mng.peers_handle();

        let forwarder = EthRequestHandler::new(client, peer_handle, eth_recv);

        let network_task = tokio::spawn(async move { network_mng.await });
        let transaction_task = tokio::spawn(async move { transaction_msg.await });

        let eth_req_handler = tokio::spawn(async move { forwarder.await });
        Self { handles: vec![network_task, transaction_task, eth_req_handler] }
        // spawn all of our tasks
    }
}

impl Future for Reconnaissance
{
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> std::task::Poll<Self::Output>
    {
        for i in &mut self.handles
        {
            match i.poll_unpin(cx)
            {
                Poll::Ready(_) => return Poll::Ready(()),
                Poll::Pending => continue
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
