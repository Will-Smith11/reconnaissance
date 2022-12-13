use std::{str::FromStr, sync::Arc, task::Poll, time::Duration};

use ethers::providers::{Http, Provider};
use futures::{stream::FuturesUnordered, Future, FutureExt, StreamExt};
use reth_discv4::NodeRecord;
use reth_interfaces::p2p::bodies::client::BodiesClient;
use reth_network::{
    eth_requests::EthRequestHandler, transactions::TransactionsManager, NetworkConfig,
    NetworkHandle, NetworkManager, PeersConfig
};
use reth_primitives::H512;
use reth_tracing::tracing::info;
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

fn node_record(str: &str) -> NodeRecord
{
    let remove_prefix = &str[8..];
    let split = remove_prefix.split('@').collect::<Vec<_>>();
    let (id, addr) = (
        H512::from_str(split.first().unwrap()).unwrap(),
        std::net::SocketAddr::from_str(split.get(1).unwrap()).unwrap()
    );

    NodeRecord { address: addr.ip(), tcp_port: addr.port(), udp_port: addr.port(), id }
}
fn get_boot_nodes() -> Vec<NodeRecord>
{
    // taken from https://github.com/ethereum/go-ethereum/blob/master/params/bootnodes.go
    vec![
	"enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303",   // bootnode-aws-ap-southeast-1-001
	"enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303",     // bootnode-aws-us-east-1-001
	"enode://8499da03c47d637b20eee24eec3c356c9a2e6148d6fe25ca195c7949ab8ec2c03e3556126b0d7ed644675e78c4318b08691b7b57de10e5f0d40d05b09238fa0a@52.187.207.27:30303",   // bootnode-azure-australiaeast-001
	"enode://103858bdb88756c71f15e9b5e09b56dc1be52f0a5021d46301dbbfb7e130029cc9d0d6f73f693bc29b665770fff7da4d34f3c6379fe12721b5d7a0bcb5ca1fc1@191.234.162.198:30303", // bootnode-azure-brazilsouth-001
	"enode://715171f50508aba88aecd1250af392a45a330af91d7b90701c436b618c86aaa1589c9184561907bebbb56439b8f8787bc01f49a7c77276c58c1b09822d75e8e8@52.231.165.108:30303",  // bootnode-azure-koreasouth-001
	"enode://5d6d7cd20d6da4bb83a1d28cadb5d409b64edf314c0335df658c1a54e32c7c4a7ab7823d57c39b6a757556e68ff1df17c748b698544a55cb488b52479a92b60f@104.42.217.25:30303",   // bootnode-azure-westus-001
	"enode://2b252ab6a1d0f971d9722cb839a42cb81db019ba44c08754628ab4a823487071b5695317c8ccd085219c3a03af063495b2f1da8d18218da2d6a82981b45e6ffc@65.108.70.101:30303",   // bootnode-hetzner-hel
	"enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@157.90.35.166:30303"  // bootnode-hetzner-fsn
    ]
    .into_iter()
    .map(node_record)
    .collect::<Vec<_>>()
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
        let provider = Arc::new(Provider::<Http>::try_from("https://rpc.ankr.com/eth").unwrap());
        let cycler = Cycler::new(vec![provider.clone()]);
        let client = Arc::new(BlockClient::new(cycler, client_sender));

        let peer_config = PeersConfig::default()
            .with_max_inbound(100)
            .with_max_outbound(100)
            .with_max_pending_inbound(5)
            .with_max_pending_outbound(5)
            .with_slot_refill_interval(Duration::from_millis(200));

        let secret_key = SecretKey::new(&mut rand::thread_rng());
        let network_config = NetworkConfig::builder(client.clone(), secret_key)
            .boot_nodes(get_boot_nodes())
            .peer_config(peer_config)
            .build();

        let mut network_mng = NetworkManager::new(network_config).await.unwrap();

        let network_handle = network_mng.handle().clone();

        let ordering = BasicOrdering;
        let fake_val = NonValidator::new(provider);
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
                            let res = client.get_block_body(req).await.map(|res| res.1);
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
                info!("have {} connected peers", self.network_handle.num_connected_peers());
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
