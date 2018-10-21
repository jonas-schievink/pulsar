//! Defines error types and codes.

use std::error;

// TODO: Make `Error` always carry a pulse error code

/// Generic error used by the library.
#[derive(Debug, Fail)]
#[fail(display = "{}", inner)]
pub struct Error {
    /*code: PulseError,
    msg: String,*/
    inner: Inner,
}

impl Error {
    pub(crate) fn string<S: AsRef<str>>(string: S) -> Self {
        Self {
            inner: Inner::Other(string.as_ref().into()),
        }
    }

    /*pub(crate) fn new<S: ToString>(code: PulseError, msg: S) -> Self {
        Self {
            code,
            msg: msg.to_string(),
        }
    }*/
}

impl<E: error::Error + Send + Sync + 'static> From<E> for Error {
    fn from(err: E) -> Error {
        Error {
            inner: Inner::Other(err.into()),
        }
    }
}

#[derive(Debug, Fail)]
enum Inner {
    #[fail(display = "{}", _0)]
    Other(Box<error::Error + Send + Sync>),
}

/// An error code understood by the PulseAudio protocol.
///
/// Can be sent to clients to inform them of a specific error.
#[repr(u32)]
#[derive(Debug, Copy, Clone, FromPrimitive, Fail)]
// TODO: Rename to `ErrorCode`?
pub enum PulseError {
    /// Access failure
    #[fail(display = "Access failure")]
    Access = 1,
    /// Unknown command
    #[fail(display = "Unknown command")]
    Command,
    /// Invalid argument
    #[fail(display = "Invalid argument")]
    Invalid,
    /// Entity exists
    #[fail(display = "Entity exists")]
    Exist,
    /// No such entity
    #[fail(display = "No such entity")]
    NoEntity,
    /// Connection refused
    #[fail(display = "Connection refused")]
    ConnectionRefused,
    /// Protocol error
    #[fail(display = "Protocol error")]
    Protocol,
    /// Timeout
    #[fail(display = "Timeout")]
    Timeout,
    /// No authentication key
    #[fail(display = "No authentication key")]
    AuthKey,
    /// Internal error
    #[fail(display = "Internal error")]
    Internal,
    /// Connection terminated
    #[fail(display = "Connection terminated")]
    ConnectionTerminated,
    /// Entity killed
    #[fail(display = "Entity killed")]
    Killed,
    /// Invalid server
    #[fail(display = "Invalid server")]
    InvalidServer,
    /// Module initialization failed
    #[fail(display = "Module initialization failed")]
    ModInitFailed,
    /// Bad state
    #[fail(display = "Bad state")]
    BadState,
    /// No data
    #[fail(display = "No data")]
    NoData,
    /// Incompatible protocol version
    #[fail(display = "Incompatible protocol version")]
    Version,
    /// Data too large
    #[fail(display = "Data too large")]
    TooLarge,
    /// Operation not supported (since 0.9.5)
    #[fail(display = "Operation not supported")]
    NotSupported,
    /// The error code was unknown to the client
    #[fail(display = "The error code was unknown to the client")]
    Unknown,
    /// Extension does not exist. (since 0.9.12)
    #[fail(display = "Extension does not exist")]
    NoExtension,
    /// Obsolete functionality. (since 0.9.15)
    #[fail(display = "Obsolete functionality")]
    Obsolete,
    /// Missing implementation. (since 0.9.15)
    #[fail(display = "Missing implementation")]
    NotImplemented,
    /// The caller forked without calling execve() and tried to reuse the context. \since 0.9.15
    #[fail(display = "The caller forked without calling execve() and tried to reuse the context")]
    Forked,
    /// An IO error happened. (since 0.9.16)
    #[fail(display = "An IO error happened")]
    Io,
    /// Device or resource busy. (since 0.9.17)
    #[fail(display = "Device or resource busy")]
    Busy,
}
