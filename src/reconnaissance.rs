use std::{
    net::{SocketAddr, SocketAddrV4},
    str::FromStr,
    sync::Arc,
    task::Poll,
    time::Duration
};

use ethers::providers::{Http, Provider};
use futures::{stream::FuturesUnordered, Future, FutureExt, StreamExt};
use reth_discv4::NodeRecord;
use reth_interfaces::p2p::bodies::client::BodiesClient;
use reth_network::{
    eth_requests::EthRequestHandler, transactions::TransactionsManager, NetworkConfig,
    NetworkHandle, NetworkManager
};
use reth_primitives::H512;
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
        let ip = "34.206.90.238:48308";
        let id = H512::from_str("59866ac8bc3308a21f2051f9f4e372617fa728b47f09fe627e7fb960f572abba64529501304ebf93b0e77b4f9010b23288bf0fa08afca595dda63854d6daea8e").unwrap();
        let ip_addr = SocketAddr::V4(SocketAddrV4::from_str(ip).unwrap());
        let node_record = NodeRecord {
            address: ip_addr.ip(),
            tcp_port: ip_addr.port(),
            udp_port: ip_addr.port(),
            id
        };
        let (client_sender, client_recv) = unbounded_channel();
        let provider = Provider::<Http>::try_from("https://rpc.ankr.com/eth").unwrap();
        let cycler = Cycler::new(vec![provider.into()]);
        let client = Arc::new(BlockClient::new(cycler, client_sender));

        let secret_key = SecretKey::new(&mut rand::thread_rng());
        let network_config = NetworkConfig::builder(client.clone(), secret_key)
            .boot_nodes(vec![node_record])
            .build();

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
            stats_interval: tokio::time::interval(Duration::from_secs(3)),
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
            Poll::Pending =>
            {}
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
