use std::sync::OnceLock;

use opendal::{Builder, Operator};
use thiserror::Error;

static SOURCE_OP: OnceLock<Operator> = OnceLock::new();

pub struct Storage;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("{0}")]
    State(String),
    #[error(transparent)]
    Operator(#[from] opendal::Error),
}

impl Storage {
    pub fn init<B: Builder>(config: B) -> Result<(), StorageError> {
        let operator = Operator::new(config)?;
        SOURCE_OP
            .set(operator)
            .map_err(|_| StorageError::State("storage is already initialized".to_owned()))
    }

    pub fn source_op() -> Result<&'static Operator, StorageError> {
        SOURCE_OP.get().ok_or_else(|| {
            StorageError::State("storage is not initialized; call Storage::init first".to_owned())
        })
    }
}
