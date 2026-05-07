//! HTTP client for sending A2A messages to a peer's `/a2a/messages` and
//! fetching its `/a2a/agent-card`.

use crate::wire::{AgentCard, Message};
use crate::Error;

#[derive(Default)]
pub struct A2aClient;

impl A2aClient {
    pub fn new() -> Self {
        A2aClient
    }

    /// POST a [`Message`] to `peer_base_url + /a2a/messages`. Returns Ok on
    /// any 2xx; an Err for non-2xx with the body as the message.
    pub fn send(&self, peer_base_url: &str, message: &Message) -> Result<(), Error> {
        let body = serde_json::to_string(message)?;
        let url = format!("{}/a2a/messages", peer_base_url.trim_end_matches('/'));
        match ureq::post(&url)
            .set("Content-Type", "application/json")
            .send_string(&body)
        {
            Ok(resp) if (200..300).contains(&resp.status()) => Ok(()),
            Ok(resp) => Err(Error::Http(format!(
                "{} {}",
                resp.status(),
                resp.into_string().unwrap_or_default()
            ))),
            Err(ureq::Error::Status(code, resp)) => Err(Error::Http(format!(
                "{} {}",
                code,
                resp.into_string().unwrap_or_default()
            ))),
            Err(e) => Err(Error::Http(e.to_string())),
        }
    }

    /// GET the peer's [`AgentCard`].
    pub fn fetch_agent_card(&self, peer_base_url: &str) -> Result<AgentCard, Error> {
        let url = format!("{}/a2a/agent-card", peer_base_url.trim_end_matches('/'));
        let resp = ureq::get(&url)
            .call()
            .map_err(|e| Error::Http(e.to_string()))?;
        let card: AgentCard = resp.into_json().map_err(|e| Error::Decode(e.to_string()))?;
        Ok(card)
    }
}
