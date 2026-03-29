use super::remote::RemoteStorage;
use anyhow::{anyhow, Result};
use shared::sync::*;

pub struct HttpRemoteClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl HttpRemoteClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn check_status(&self, status: u16) -> Result<()> {
        if !status.to_string().starts_with('2') {
            return Err(anyhow!("HTTP error: {}", status));
        }
        Ok(())
    }
}

impl RemoteStorage for HttpRemoteClient {
    fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>> {
        let url = format!("{}/sync/events", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(events)
            .send()?;

        self.check_status(response.status().as_u16())?;
        let sequences: Vec<u64> = response.json()?;
        Ok(sequences)
    }

    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>> {
        let url = format!(
            "{}/sync/events?child_id={}&since={}",
            self.base_url, child_id, since_sequence
        );
        let response = self.client.get(&url).send()?;

        self.check_status(response.status().as_u16())?;
        let events: Vec<SyncEvent> = response.json()?;
        Ok(events)
    }

    fn upsert_entity(
        &self,
        child_id: &str,
        entity_type: EntityType,
        entity_id: &str,
        entity_json: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/entities/{}/{}/{}",
            self.base_url,
            entity_type.as_str(),
            child_id,
            entity_id
        );
        let response = self
            .client
            .put(&url)
            .body(entity_json.to_string())
            .send()?;

        self.check_status(response.status().as_u16())?;
        Ok(())
    }

    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>> {
        let url = format!(
            "{}/entities/{}/{}/{}",
            self.base_url,
            entity_type.as_str(),
            child_id,
            entity_id
        );
        let response = self.client.get(&url).send()?;

        match response.status().as_u16() {
            404 => Ok(None),
            status if status.to_string().starts_with('2') => {
                let body = response.text()?;
                Ok(Some(body))
            }
            status => Err(anyhow!("HTTP error: {}", status)),
        }
    }

    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()> {
        let url = format!(
            "{}/entities/{}/{}/{}",
            self.base_url,
            entity_type.as_str(),
            child_id,
            entity_id
        );
        let response = self.client.delete(&url).send()?;

        self.check_status(response.status().as_u16())?;
        Ok(())
    }

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint> {
        let url = format!("{}/sync/checkpoint/{}", self.base_url, child_id);
        let response = self.client.get(&url).send()?;

        self.check_status(response.status().as_u16())?;
        let checkpoint: SyncCheckpoint = response.json()?;
        Ok(checkpoint)
    }

    fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> Result<()> {
        let url = format!("{}/sync/checkpoint/{}", self.base_url, child_id);
        let payload = serde_json::json!({
            "which": which,
            "value": value
        });
        let response = self
            .client
            .put(&url)
            .json(&payload)
            .send()?;

        self.check_status(response.status().as_u16())?;
        Ok(())
    }

    fn initialize_child(&self, child_id: &str) -> Result<()> {
        let url = format!("{}/sync/initialize/{}", self.base_url, child_id);
        let response = self.client.post(&url).send()?;

        self.check_status(response.status().as_u16())?;
        Ok(())
    }

    fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send()?;

        match response.status().as_u16() {
            200 => Ok(true),
            _ => Ok(false),
        }
    }
}
