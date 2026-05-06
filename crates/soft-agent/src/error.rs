//! Error types for soft-agent.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    InvalidConfig(String),
    HandlerNotRegistered(String),
    MailboxClosed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidConfig(msg) => write!(f, "invalid agent config: {msg}"),
            Error::HandlerNotRegistered(topic) => write!(f, "no handler for topic {topic}"),
            Error::MailboxClosed => write!(f, "mailbox closed"),
        }
    }
}

impl std::error::Error for Error {}
