use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("connect: {0}")]
    Connect(#[from] ConnectError),
    #[error("persist: {0}")]
    Persist(#[from] PersistError),
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("placeholder")]
    Placeholder,
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("placeholder")]
    Placeholder,
}
