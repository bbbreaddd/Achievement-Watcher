pub mod domain;
pub mod gog;
pub mod migration;
pub mod parser;
mod secure;
pub mod source;
pub mod store;

pub use domain::*;
pub use store::Store;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported achievement file: {0}")]
    Unsupported(String),
    #[error("invalid achievement data: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
