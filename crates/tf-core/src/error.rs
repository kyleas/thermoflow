use thiserror::Error;

pub type TfResult<T> = Result<T, TfError>;

#[derive(Error, Debug)]
pub enum TfError {
    #[error("Non-finite numeric value for {what}: {value}")]
    NonFinite { what: &'static str, value: f64 },

    #[error("Invalid argument: {what}")]
    InvalidArg { what: &'static str },

    #[error("Index out of bounds: {what} (index={index}, len={len})")]
    IndexOob {
        what: &'static str,
        index: usize,
        len: usize,
    },

    #[error("Invariant violated: {what}")]
    Invariant { what: &'static str },
}
