//! Error types for soft-a2a.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Bind(String),
    Http(String),
    Encode(String),
    Decode(String),
    BadRequest(String),
    MailboxClosed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Bind(s) => write!(f, "bind: {s}"),
            Error::Http(s) => write!(f, "http: {s}"),
            Error::Encode(s) => write!(f, "encode: {s}"),
            Error::Decode(s) => write!(f, "decode: {s}"),
            Error::BadRequest(s) => write!(f, "bad request: {s}"),
            Error::MailboxClosed => write!(f, "mailbox closed"),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Encode(e.to_string())
    }
}
