use thiserror::Error;

#[derive(Debug, Error)]
pub enum CtlError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("file read failed: {0}")]
    FileReadFailed(String),

    #[error("file write failed: {0}")]
    FileWriteFailed(String),

    #[error("binary not found: {0}")]
    BinaryNotFound(String),

    #[error("process launch failed: {0}")]
    ProcessLaunchFailed(String),

    #[error("integration failed: {0}")]
    IntegrationFailed(String),
}

impl CtlError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidArgument(_) => 2,
            Self::FileReadFailed(_) | Self::FileWriteFailed(_) => 3,
            Self::BinaryNotFound(_) => 4,
            Self::ProcessLaunchFailed(_) => 5,
            Self::IntegrationFailed(_) => 6,
        }
    }
}
