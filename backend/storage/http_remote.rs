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

    fn check_response(&self, url: &str, response: reqwest::blocking::Response) -> Result<reqwest::blocking::Response> {
        let status = response.status().as_u16();
        if status < 200 || status >= 300 {
            let body = response.text().unwrap_or_default();
            return Err(anyhow!("HTTP {} from {}: {}", status, url, body));
        }
        Ok(response)
    }
}

impl RemoteStorage for HttpRemoteClient {
    fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>> {
        let url = format!("{}/sync/events", self.base_url);

        #[derive(serde::Serialize)]
        struct PushRequest<'a> {
            events: &'a [SyncEvent],
        }

        #[derive(serde::Deserialize)]
        struct PushResponse {
            sequences: Vec<u64>,
        }

        let response = self
            .client
            .post(&url)
            .json(&PushRequest { events })
            .send()?;

        let response = self.check_response(&url, response)?;
        let parsed: PushResponse = response.json()?;
        Ok(parsed.sequences)
    }

    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>> {
        let url = format!(
            "{}/sync/events?child_id={}&since={}",
            self.base_url, child_id, since_sequence
        );
        let response = self.client.get(&url).send()?;

        let response = self.check_response(&url, response)?;
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

        self.check_response(&url, response)?;
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
            status if (200..300).contains(&status) => {
                let body = response.text()?;
                Ok(Some(body))
            }
            status => {
                let body = response.text().unwrap_or_default();
                Err(anyhow!("HTTP {} from {}: {}", status, url, body))
            }
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

        self.check_response(&url, response)?;
        Ok(())
    }

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint> {
        let url = format!("{}/sync/checkpoint/{}", self.base_url, child_id);
        let response = self.client.get(&url).send()?;

        let response = self.check_response(&url, response)?;
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

        self.check_response(&url, response)?;
        Ok(())
    }

    fn initialize_child(&self, child_id: &str) -> Result<()> {
        let url = format!("{}/sync/initialize/{}", self.base_url, child_id);
        let response = self.client.post(&url).send()?;

        self.check_response(&url, response)?;
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
