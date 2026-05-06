//! Error types for soft-agent.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    InvalidConfig(String),
    HandlerNotRegistered(String),
    MailboxClosed,
    Spec(String),
    Trace(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidConfig(msg) => write!(f, "invalid agent config: {msg}"),
            Error::HandlerNotRegistered(topic) => write!(f, "no handler for topic {topic}"),
            Error::MailboxClosed => write!(f, "mailbox closed"),
            Error::Spec(msg) => write!(f, "spec error: {msg}"),
            Error::Trace(msg) => write!(f, "trace error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
