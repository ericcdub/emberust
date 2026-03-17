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
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "userName": self.username,
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
        // Token validity is around 30 minutes based on pyephember reference
        self.token_expiry = Some(Instant::now() + Duration::from_secs(1800));
        Ok(())
    }

    async fn refresh_access_token(&mut self) -> Result<()> {
        let refresh = self
            .refresh_token
            .clone()
            .ok_or_else(|| anyhow!("No refresh token"))?;

        let resp = self
            .client
            .get(format!("{BASE_URL}/appLogin/refreshAccessToken"))
            .header("Authorization", &refresh)
            .send()
            .await?
            .json::<ApiResponse<LoginData>>()
            .await?;

        let data = resp
            .data
            .ok_or_else(|| anyhow!("Token refresh failed"))?;

        self.access_token = Some(data.token);
        self.refresh_token = Some(data.refresh_token);
        self.token_expiry = Some(Instant::now() + Duration::from_secs(1800));
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
            .get(format!("{BASE_URL}/homes/list"))
            .header("Authorization", &token)
            .header("Accept", "application/json")
            .send()
            .await?
            .json::<ApiResponse<Vec<Home>>>()
            .await?;

        resp.data.ok_or_else(|| anyhow!("Failed to list homes"))
    }

    pub async fn get_zones(&mut self) -> Result<Vec<Zone>> {
        self.ensure_auth().await?;
        let token = self.auth_header()?;
        let homes = self.list_homes().await?;

        log::info!("Found {} homes/gateways", homes.len());

        let mut all_zones = Vec::new();

        // Fetch zones from all homes/gateways
        for home in &homes {
            log::info!("Fetching zones for gateway: {} ({})", home.name, home.gateway_id);
            
            // First get home details to get productId and uid
            let details_resp = self
                .client
                .post(format!("{BASE_URL}/homes/detail"))
                .header("Authorization", &token)
                .json(&serde_json::json!({ "gateWayId": home.gateway_id }))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            
            let product_id = details_resp["data"]["homes"]["productId"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let uid = details_resp["data"]["homes"]["uid"]
                .as_str()
                .unwrap_or("")
                .to_string();
            
            log::info!("Home details: productId={}, uid={}", product_id, uid);
            
            // Now get zone program
            let response = self
                .client
                .post(format!("{BASE_URL}/homesVT/zoneProgram"))
                .header("Authorization", &token)
                .json(&serde_json::json!({ "gateWayId": home.gateway_id }))
                .send()
                .await?;

            let body = response.text().await?;
            log::debug!("Raw response: {}", body);

            let resp: ApiResponse<Vec<Zone>> = match serde_json::from_str(&body) {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Failed to parse zones response: {}", e);
                    log::error!("Response body: {}", body);
                    continue;
                }
            };

            if let Some(mut zones) = resp.data {
                log::info!("Gateway {} returned {} zones", home.name, zones.len());
                // Set productId and uid from home details on each zone
                for z in &mut zones {
                    z.product_id = product_id.clone();
                    z.uid = uid.clone();
                    log::info!("  - Zone: {} (mac: {}, productId: {}, uid: {})", 
                        z.name, z.mac, z.product_id, z.uid);
                }
                all_zones.extend(zones);
            } else {
                log::warn!("Gateway {} returned no zones data", home.name);
            }
        }

        log::info!("Total zones found: {}", all_zones.len());
        self.zones = all_zones.clone();
        Ok(all_zones)
    }

    /// Authenticate and fetch initial zone data in one call.
    pub async fn login_and_fetch(&mut self) -> Result<Vec<Zone>> {
        self.login().await?;
        self.get_user_id().await?;
        self.get_zones().await
    }

    pub fn mqtt_credentials(&self) -> Option<MqttCredentials> {
        // MQTT uses the access token (not refresh_token) per pyephember reference
        let token = self.access_token.as_ref()?;
        let user_id = self.user_id?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis();

        Some(MqttCredentials {
            client_id: format!("{user_id}_{ts}"),
            username: format!("app/{token}"),
            password: token.clone(),
            user_id,
        })
    }

    pub fn find_zone(&self, name: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.name == name)
    }

    pub fn cached_zones(&self) -> &[Zone] {
        &self.zones
    }
}
