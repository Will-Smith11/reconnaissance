use reth_primitives::{FromRecoveredTransaction, TransactionSignedEcRecovered};
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

impl TransactionValidator for NonValidator
{
    type Transaction = TxPoolTx;

    fn validate_transaction<'life0, 'async_trait>(
        &'life0 self,
        origin: TransactionOrigin,
        _transaction: Self::Transaction
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = TransactionValidationOutcome<Self::Transaction>>
                + core::marker::Send
                + 'async_trait
        >
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        todo!()
    }
}
