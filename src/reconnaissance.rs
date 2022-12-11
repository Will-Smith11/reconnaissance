use std::{sync::Arc, task::Poll, time::Duration};

use ethers::providers::{Http, Provider};
use futures::{stream::FuturesUnordered, Future, FutureExt, StreamExt};
use reth_interfaces::p2p::bodies::client::BodiesClient;
use reth_network::{
    eth_requests::EthRequestHandler, transactions::TransactionsManager, NetworkConfig,
    NetworkHandle, NetworkManager
};
use reth_transaction_pool::{Pool, PoolConfig};
use secp256k1::SecretKey;
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver},
    task::JoinHandle,
    time::Interval
};

use crate::{
    block_client::{BlockClient, Cycler, FetcherReq},
    tx::{BasicOrdering, NonValidator}
};

pub fn get_cycler() -> Cycler<Arc<Provider<Http>>>
{
    let provider = Provider::<Http>::try_from("").unwrap();
    Cycler::new(vec![provider.into()])
}
/// handles all of our forwarding
pub struct Reconnaissance
{
    network_handle: NetworkHandle,
    stats_interval: Interval,
    handles:        Vec<JoinHandle<()>>,
    client_event:   UnboundedReceiver<FetcherReq>,
    pending_tasks:  FuturesUnordered<JoinHandle<()>>
}

impl Reconnaissance
{
    pub async fn new() -> Self
    {
        let (client_sender, client_recv) = unbounded_channel();
        let client = Arc::new(BlockClient::new(get_cycler(), client_sender));

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
        let transaction_msg =
            TransactionsManager::new(network_handle.clone(), transaction_pool, tx_recv);
        let peer_handle = network_mng.peers_handle();

        let forwarder = EthRequestHandler::new(client, peer_handle, eth_recv);

        let network_task = tokio::spawn(async move { network_mng.await });
        let transaction_task = tokio::spawn(async move { transaction_msg.await });

        let eth_req_handler = tokio::spawn(async move { forwarder.await });
        Self {
            handles: vec![network_task, transaction_task, eth_req_handler],
            network_handle,
            stats_interval: tokio::time::interval(Duration::from_secs(10)),
            client_event: client_recv,
            pending_tasks: FuturesUnordered::default()
        }
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
        // queue up new tasks
        match self.client_event.poll_recv(cx)
        {
            Poll::Ready(data) =>
            {
                let Some(data) = data else { return Poll::Ready(())};
                match data
                {
                    FetcherReq::BlockBodyReq((channel, req)) =>
                    {
                        let new_network_handle = self.network_handle.clone();
                        let task = tokio::task::spawn(async move {
                            let client = new_network_handle.fetch_client().await.unwrap();
                            let res = client.get_block_body(req).await;
                            channel.send(res).unwrap();
                        });

                        self.pending_tasks.push(task)
                    }
                }
            }
            Poll::Pending => todo!()
        }

        // poll all pending tasks
        while let Poll::Ready(Some(_)) = self.pending_tasks.poll_next_unpin(cx)
        {}

        match self.stats_interval.poll_tick(cx)
        {
            Poll::Ready(_) =>
            {
                println!("have {} connected peers", self.network_handle.num_connected_peers());
            }
            Poll::Pending =>
            {}
        }
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
