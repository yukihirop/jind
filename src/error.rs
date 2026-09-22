use thiserror::Error;

#[derive(Debug, Error)]
pub enum JindError {
    #[error("{0}")]
    Usage(String),
    #[error("could not interpret: {0}")]
    Unresolved(String),
    #[error("conflicting words: {0}")]
    Conflict(String),
    #[error("jev: {0}")]
    Jev(String),
    #[error("interpretation rejected (confidence {0:.2} < {1:.2}); rerun with --explain to see why")]
    LowConfidence(f32, f32),
    #[error("aborted")]
    Aborted,
    #[error("find not found in PATH")]
    FindMissing,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("config: {0}")]
    Config(String),
}

impl JindError {
    pub fn exit_code(&self) -> i32 {
        match self {
            JindError::FindMissing => 127,
            JindError::Usage(_) | JindError::Config(_) => 64,
            _ => 2,
        }
    }
}
