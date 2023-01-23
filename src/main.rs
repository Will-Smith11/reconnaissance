use reconnaissance::reconnaissance::Reconnaissance;

#[tokio::main(flavor = "multi_thread")]
async fn main()
{
    fdlimit::raise_fd_limit();
    reth_tracing::init_test_tracing();
    let reconnaissance = Reconnaissance::new().await;
    reconnaissance.await
}
