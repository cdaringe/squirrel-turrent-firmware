use esp_idf_svc::io::EspIOError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("server io failure")]
    IOError(#[from] EspIOError),
    #[error("[de]serialization error")]
    SerdeError(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    BadInput(String),
    #[error("unknown: {0}")]
    Unknown(String),
}
