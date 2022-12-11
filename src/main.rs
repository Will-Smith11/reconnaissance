use reconnaissance::reconnaissance::Reconnaissance;

#[tokio::main(flavor = "multi_thread")]
async fn main()
{
    reth_tracing::init_tracing();
    let reconnaissance = Reconnaissance::new().await;
    reconnaissance.await
}
