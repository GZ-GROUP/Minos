//! Error type shared by every app command. Serialized as `{ code, message }`; the
//! frontend translates `code` and may show `message` as technical detail.

use std::io;

use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    Exists,
    PermissionDenied,
    InvalidName,
    InvalidInput,
    OutsideHome,
    NotEmpty,
    InvalidPdf,
    ConversionFailed,
    UnsupportedFormat,
    Unreachable,
    AuthFailed,
    HostKeyChanged,
    NotConnected,
    Timeout,
    Io,
}

#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        let code = match error.kind() {
            io::ErrorKind::NotFound => ErrorCode::NotFound,
            io::ErrorKind::AlreadyExists => ErrorCode::Exists,
            io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            io::ErrorKind::DirectoryNotEmpty => ErrorCode::NotEmpty,
            io::ErrorKind::TimedOut => ErrorCode::Timeout,
            _ => ErrorCode::Io,
        };
        Self::new(code, error.to_string())
    }
}

impl From<lopdf::Error> for AppError {
    fn from(error: lopdf::Error) -> Self {
        match error {
            lopdf::Error::IO(io) => io.into(),
            other => Self::new(ErrorCode::InvalidPdf, other.to_string()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
