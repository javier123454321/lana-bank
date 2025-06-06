use thiserror::Error;

#[derive(Error, Debug)]
pub enum StatementError {
    #[error("StatementError - ConversionError: {0}")]
    ConversionError(#[from] core_money::ConversionError),
    #[error("StatementError - ParseCurrencyError: {0}")]
    ParseCurrencyError(#[from] cala_ledger::ParseCurrencyError),
}
