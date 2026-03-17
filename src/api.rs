use crate::models::*;
use anyhow::{anyhow, Result};
use reqwest::Client;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BASE_URL: &str = "https://eu-https.topband-cloud.com/ember-back";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct EphEmberApi {
    client: Client,
    username: String,
    password: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_expiry: Option<Instant>,
    user_id: Option<u64>,
    gateway_id: Option<String>,
    zones: Vec<Zone>,
}

impl EphEmberApi {
    pub fn new(username: String, password: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("failed to create HTTP client"),
            username,
            password,
            access_token: None,
            refresh_token: None,
            token_expiry: None,
            user_id: None,
            gateway_id: None,
            zones: Vec::new(),
        }
    }

    async fn ensure_auth(&mut self) -> Result<()> {
        if let Some(expiry) = self.token_expiry {
            if Instant::now() < expiry - Duration::from_secs(30) {
                return Ok(());
            }
            if self.refresh_token.is_some() {
                if self.refresh_access_token().await.is_ok() {
                    return Ok(());
                }
            }
        }
        self.login().await
    }

    pub async fn login(&mut self) -> Result<()> {
        let resp = self
            .client
            .post(format!("{BASE_URL}/appLogin/login"))
            .json(&serde_json::json!({
                "username": self.username,
                "password": self.password,
            }))
            .send()
            .await?
            .json::<ApiResponse<LoginData>>()
            .await?;

        let data = resp
            .data
            .ok_or_else(|| anyhow!("Login failed: {}", resp.message.unwrap_or_default()))?;

        self.access_token = Some(data.token);
        self.refresh_token = Some(data.refresh_token);
        self.token_expiry = Some(Instant::now() + Duration::from_secs(data.expires_in));
        Ok(())
    }

    async fn refresh_access_token(&mut self) -> Result<()> {
        let token = self.auth_header()?;

        let resp = self
            .client
            .get(format!("{BASE_URL}/appLogin/refreshAccessToken"))
            .header("Authorization", &token)
            .send()
            .await?
            .json::<ApiResponse<LoginData>>()
            .await?;

        let data = resp
            .data
            .ok_or_else(|| anyhow!("Token refresh failed"))?;

        self.access_token = Some(data.token);
        self.refresh_token = Some(data.refresh_token);
        self.token_expiry = Some(Instant::now() + Duration::from_secs(data.expires_in));
        Ok(())
    }

    fn auth_header(&self) -> Result<String> {
        self.access_token
            .clone()
            .ok_or_else(|| anyhow!("Not authenticated"))
    }

    pub async fn get_user_id(&mut self) -> Result<u64> {
        if let Some(id) = self.user_id {
            return Ok(id);
        }
        self.ensure_auth().await?;
        let token = self.auth_header()?;

        let resp = self
            .client
            .get(format!("{BASE_URL}/user/selectUser"))
            .header("Authorization", &token)
            .send()
            .await?
            .json::<ApiResponse<UserData>>()
            .await?;

        let data = resp
            .data
            .ok_or_else(|| anyhow!("Failed to get user details"))?;
        self.user_id = Some(data.id);
        Ok(data.id)
    }

    pub async fn list_homes(&mut self) -> Result<Vec<Home>> {
        self.ensure_auth().await?;
        let token = self.auth_header()?;

        let resp = self
            .client
            .post(format!("{BASE_URL}/homes/list"))
            .header("Authorization", &token)
            .json(&serde_json::json!({}))
            .send()
            .await?
            .json::<ApiResponse<Vec<Home>>>()
            .await?;

        resp.data.ok_or_else(|| anyhow!("Failed to list homes"))
    }

    async fn ensure_gateway(&mut self) -> Result<String> {
        if let Some(ref id) = self.gateway_id {
            return Ok(id.clone());
        }
        let homes = self.list_homes().await?;
        let gw = homes
            .first()
            .ok_or_else(|| anyhow!("No gateways found"))?
            .gateway_id
            .clone();
        self.gateway_id = Some(gw.clone());
        Ok(gw)
    }

    pub async fn get_zones(&mut self) -> Result<Vec<Zone>> {
        self.ensure_auth().await?;
        let token = self.auth_header()?;
        let gateway_id = self.ensure_gateway().await?;

        let resp = self
            .client
            .post(format!("{BASE_URL}/homesVT/zoneProgram"))
            .header("Authorization", &token)
            .json(&serde_json::json!({ "gateWayId": gateway_id }))
            .send()
            .await?
            .json::<ApiResponse<Vec<Zone>>>()
            .await?;

        let zones = resp.data.ok_or_else(|| anyhow!("Failed to get zones"))?;
        self.zones = zones.clone();
        Ok(zones)
    }

    /// Authenticate and fetch initial zone data in one call.
    pub async fn login_and_fetch(&mut self) -> Result<Vec<Zone>> {
        self.login().await?;
        self.get_user_id().await?;
        self.get_zones().await
    }

    pub fn mqtt_credentials(&self) -> Option<MqttCredentials> {
        let refresh_token = self.refresh_token.as_ref()?;
        let user_id = self.user_id?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis();

        Some(MqttCredentials {
            client_id: format!("{user_id}_{ts}"),
            username: format!("app/{refresh_token}"),
            password: refresh_token.clone(),
        })
    }

    pub fn find_zone(&self, name: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.name == name)
    }

    pub fn cached_zones(&self) -> &[Zone] {
        &self.zones
    }
}
