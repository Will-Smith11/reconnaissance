use reth_primitives::{
    FromRecoveredTransaction, IntoRecoveredTransaction, TransactionSignedEcRecovered
};
use reth_transaction_pool::{
    PoolTransaction, TransactionOrdering, TransactionOrigin, TransactionValidationOutcome,
    TransactionValidator
};
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
        todo!()
    }

    fn sender(&self) -> &reth_primitives::Address
    {
        todo!()
    }

    fn nonce(&self) -> u64
    {
        todo!()
    }

    fn cost(&self) -> reth_primitives::U256
    {
        todo!()
    }

    fn effective_gas_price(&self) -> reth_primitives::U256
    {
        todo!()
    }

    fn gas_limit(&self) -> u64
    {
        todo!()
    }

    fn max_fee_per_gas(&self) -> Option<reth_primitives::U256>
    {
        todo!()
    }

    fn max_priority_fee_per_gas(&self) -> Option<reth_primitives::U256>
    {
        todo!()
    }

    fn size(&self) -> usize
    {
        todo!()
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

pub struct NonValidator;

#[async_trait::async_trait]
impl TransactionValidator for NonValidator
{
    type Transaction = TxPoolTx;

    async fn validate_transaction(
        &self,
        origin: TransactionOrigin,
        _transaction: Self::Transaction
    ) -> TransactionValidationOutcome<Self::Transaction>
    {
        todo!()
    }
}
