use reth_primitives::{
    FromRecoveredTransaction, IntoRecoveredTransaction, TransactionSignedEcRecovered, U256
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
            reth_primitives::Transaction::Legacy(t) => t.gas_price.into(),
            reth_primitives::Transaction::Eip2930(t) => t.gas_price.into(),
            reth_primitives::Transaction::Eip1559(t) => t.max_fee_per_gas.into()
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
            reth_primitives::Transaction::Eip1559(t) => Some(t.max_fee_per_gas.into()),
            _ => None
        }
    }

    fn max_priority_fee_per_gas(&self) -> Option<reth_primitives::U256>
    {
        match &self.inner.transaction
        {
            reth_primitives::Transaction::Eip1559(t) => Some(t.max_priority_fee_per_gas.into()),
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
        match origin
        {
            TransactionOrigin::Local => todo!(),
            TransactionOrigin::External => todo!()
        }
    }
}
