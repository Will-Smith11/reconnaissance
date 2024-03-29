use std::sync::Arc;

use ethers::providers::{JsonRpcClient, Middleware, Provider};
use futures::FutureExt;
use reth_primitives::{
    FromRecoveredTransaction, IntoRecoveredTransaction, TransactionSignedEcRecovered, U256
};
use reth_tracing::tracing::{debug, error};
use reth_transaction_pool::{
    PoolTransaction, TransactionOrdering, TransactionOrigin, TransactionValidationOutcome,
    TransactionValidator
};
use tokio::join;
pub struct BasicOrdering;

#[derive(Debug)]
pub struct TxPoolTx
{
    inner: TransactionSignedEcRecovered
}

impl FromRecoveredTransaction for TxPoolTx
{
    fn from_recovered_transaction(tx: TransactionSignedEcRecovered) -> Self
    {
        Self { inner: tx }
    }
}
impl IntoRecoveredTransaction for TxPoolTx
{
    fn to_recovered_transaction(&self) -> TransactionSignedEcRecovered
    {
        self.inner.clone()
    }
}

impl PoolTransaction for TxPoolTx
{
    fn hash(&self) -> &reth_primitives::TxHash
    {
        &self.inner.hash
    }

    fn sender(&self) -> reth_primitives::Address
    {
        self.inner.signer()
    }

    fn nonce(&self) -> u64
    {
        self.inner.nonce()
    }

    fn cost(&self) -> reth_primitives::U256
    {
        match &self.inner.transaction
        {
            reth_primitives::Transaction::Legacy(t) =>
            {
                U256::from(t.gas_limit as u128 * t.gas_price * t.value)
            }
            reth_primitives::Transaction::Eip2930(t) =>
            {
                U256::from(t.gas_limit as u128 * t.gas_price * t.value)
            }
            reth_primitives::Transaction::Eip1559(t) =>
            {
                U256::from(t.gas_limit as u128 * t.max_fee_per_gas + t.value)
            }
        }
    }

    fn effective_gas_price(&self) -> reth_primitives::U256
    {
        match &self.inner.transaction
        {
            reth_primitives::Transaction::Legacy(t) => U256::from(t.gas_price),
            reth_primitives::Transaction::Eip2930(t) => U256::from(t.gas_price),
            reth_primitives::Transaction::Eip1559(t) => U256::from(t.max_fee_per_gas)
        }
    }

    fn gas_limit(&self) -> u64
    {
        self.inner.gas_limit()
    }

    fn max_fee_per_gas(&self) -> Option<reth_primitives::U256>
    {
        match &self.inner.transaction
        {
            reth_primitives::Transaction::Eip1559(t) => Some(U256::from(t.max_fee_per_gas)),
            _ => None
        }
    }

    fn max_priority_fee_per_gas(&self) -> Option<reth_primitives::U256>
    {
        match &self.inner.transaction
        {
            reth_primitives::Transaction::Eip1559(t) =>
            {
                Some(U256::from(t.max_priority_fee_per_gas))
            }
            _ => None
        }
    }

    fn size(&self) -> usize
    {
        std::mem::size_of_val(self)
    }
}

impl TransactionOrdering for BasicOrdering
{
    type Priority = u128;
    type Transaction = TxPoolTx;

    fn priority(&self, transaction: &Self::Transaction) -> Self::Priority
    {
        transaction.inner.transaction.max_fee_per_gas()
    }
}

pub struct NonValidator<P: JsonRpcClient>
{
    provider: Arc<Provider<P>>
}

impl<P: JsonRpcClient> NonValidator<P>
{
    pub fn new(provider: Arc<Provider<P>>) -> Self
    {
        Self { provider }
    }
}

#[async_trait::async_trait]
/// reth verifies the signature of the transaction for us so we will always know
/// that it is a valid transaction.. NOTE: because of the transaction pool not
/// being fully implemented on reth. there are a bunch of errors that can come
/// up
impl<P: JsonRpcClient + 'static> TransactionValidator for NonValidator<P>
{
    type Transaction = TxPoolTx;

    async fn validate_transaction(
        &self,
        _origin: TransactionOrigin,
        tx: Self::Transaction
    ) -> TransactionValidationOutcome<Self::Transaction>
    {
        // get sender nonce
        let nonce = self
            .provider
            .get_transaction_count(
                ethers::types::NameOrAddress::Address(tx.sender().0.into()),
                None
            )
            .map(|f| f.map(|t| t.as_u64()));
        let balance = self
            .provider
            .get_balance(ethers::types::NameOrAddress::Address(tx.sender().0.into()), None);

        let (Ok(nonce), Ok(bal)) = join!(nonce, balance)
        else
        {
            error!("failed to get nonce or balance");
            let hash = *tx.hash();
            return TransactionValidationOutcome::Invalid(
                tx,
                reth_transaction_pool::error::PoolError::DiscardedOnInsert(hash)
            )
        };

        if tx.nonce() != nonce
        {
            let hash = *tx.hash();
            return TransactionValidationOutcome::Invalid(
                tx,
                reth_transaction_pool::error::PoolError::DiscardedOnInsert(hash)
            )
        }

        debug!("successfully validated tx");

        TransactionValidationOutcome::Valid {
            balance:     bal.into(),
            state_nonce: nonce,
            transaction: tx
        }
    }
}
