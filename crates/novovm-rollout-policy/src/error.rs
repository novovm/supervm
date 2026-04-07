use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum PolicyError {
    InvalidArgument(String),
    LaunchToolFailed(String),
}

impl PolicyError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidArgument(_) => 2,
            Self::LaunchToolFailed(_) => 4,
        }
    }
}

impl Display for PolicyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument(v) => write!(f, "invalid argument: {v}"),
            Self::LaunchToolFailed(v) => write!(f, "launch tool failed: {v}"),
        }
    }
}

impl std::error::Error for PolicyError {}
