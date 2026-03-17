use serde::{Deserialize, Serialize};

// --- Enums ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ZoneMode {
    Auto = 0,
    AllDay = 1,
    On = 2,
    Off = 3,
}

impl ZoneMode {
    pub const ALL: [ZoneMode; 4] = [
        ZoneMode::Auto,
        ZoneMode::AllDay,
        ZoneMode::On,
        ZoneMode::Off,
    ];

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Auto),
            1 => Some(Self::AllDay),
            2 => Some(Self::On),
            3 => Some(Self::Off),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::AllDay => "All Day",
            Self::On => "On",
            Self::Off => "Off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PointIndex {
    AdvanceActive = 4,
    CurrentTemp = 5,
    TargetTemp = 6,
    Mode = 7,
    BoostHours = 8,
    BoostTime = 9,
    BoilerState = 10,
    BoostTemp = 14,
}

impl PointIndex {
    /// Returns the point data type ID and byte length for encoding commands.
    pub fn command_type(&self) -> (u8, usize) {
        match self {
            Self::AdvanceActive | Self::Mode | Self::BoostHours => (1, 1),
            Self::TargetTemp | Self::BoostTemp => (4, 2),
            Self::BoostTime => (5, 4),
            _ => (0, 0), // read-only
        }
    }
}

// --- API data structures ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointData {
    #[serde(rename = "pointIndex")]
    pub point_index: u8,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulePeriod {
    #[serde(rename = "startTime")]
    pub start_time: u32,
    #[serde(rename = "endTime")]
    pub end_time: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDay {
    #[serde(rename = "dayType")]
    pub day_type: u8,
    pub p1: SchedulePeriod,
    pub p2: SchedulePeriod,
    pub p3: SchedulePeriod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    pub mac: String,
    #[serde(rename = "zoneid", default)]
    pub zone_id: String,
    #[serde(rename = "deviceType", default)]
    pub device_type: u32,
    #[serde(rename = "productId", default)]
    pub product_id: String,
    #[serde(default)]
    pub uid: String,
    #[serde(rename = "isonline", default)]
    pub is_online: bool,
    #[serde(default)]
    pub timestamp: Option<u64>,
    #[serde(rename = "pointDataList", default)]
    pub point_data_list: Vec<PointData>,
    #[serde(rename = "deviceDays", default)]
    pub device_days: Vec<DeviceDay>,
}

impl Zone {
    pub fn point_value(&self, index: PointIndex) -> Option<&str> {
        self.point_data_list
            .iter()
            .find(|p| p.point_index == index as u8)
            .map(|p| p.value.as_str())
    }

    pub fn point_value_u32(&self, index: PointIndex) -> Option<u32> {
        self.point_value(index)?.parse().ok()
    }

    pub fn current_temperature(&self) -> Option<f32> {
        self.point_value_u32(PointIndex::CurrentTemp)
            .map(|v| v as f32 / 10.0)
    }

    pub fn target_temperature(&self) -> Option<f32> {
        self.point_value_u32(PointIndex::TargetTemp)
            .map(|v| v as f32 / 10.0)
    }

    pub fn boost_temperature(&self) -> Option<f32> {
        self.point_value_u32(PointIndex::BoostTemp)
            .map(|v| v as f32 / 10.0)
    }

    pub fn mode(&self) -> Option<ZoneMode> {
        self.point_value_u32(PointIndex::Mode)
            .and_then(|v| ZoneMode::from_u8(v as u8))
    }

    pub fn is_active(&self) -> bool {
        self.current_temperature()
            .zip(self.target_temperature())
            .map(|(cur, tgt)| cur < tgt)
            .unwrap_or(false)
    }

    pub fn is_boost_active(&self) -> bool {
        self.boost_hours().map(|h| h > 0).unwrap_or(false)
    }

    pub fn boost_hours(&self) -> Option<u32> {
        self.point_value_u32(PointIndex::BoostHours)
    }

    pub fn is_boiler_on(&self) -> bool {
        self.point_value_u32(PointIndex::BoilerState) == Some(2)
    }

    pub fn is_advance_active(&self) -> bool {
        self.point_value_u32(PointIndex::AdvanceActive) == Some(1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Home {
    pub name: String,
    #[serde(rename = "gatewayid")]
    pub gateway_id: String,
    #[serde(rename = "deviceType", default)]
    pub device_type: u32,
    #[serde(rename = "productId", default)]
    pub product_id: String,
    #[serde(default)]
    pub uid: String,
    #[serde(rename = "zoneCount", default)]
    pub zone_count: Option<u32>,
}

// --- API response wrappers ---

#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    pub data: Option<T>,
    pub status: i32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginData {
    pub token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserData {
    pub id: u64,
}

// --- Channel messages ---

#[derive(Debug)]
pub enum Command {
    Login {
        username: String,
        password: String,
    },
    RefreshZones,
    SetTargetTemperature {
        zone_name: String,
        temperature: f32,
    },
    SetMode {
        zone_name: String,
        mode: ZoneMode,
    },
    ActivateBoost {
        zone_name: String,
        temperature: Option<f32>,
        hours: u32,
    },
    DeactivateBoost {
        zone_name: String,
    },
}

#[derive(Debug)]
pub enum Update {
    LoggedIn,
    LoginFailed(String),
    ZonesUpdated(Vec<Zone>),
    Error(String),
    CommandSent(String),
}

// --- MQTT ---

#[derive(Debug, Clone)]
pub struct MqttCredentials {
    pub client_id: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct ZoneCommand {
    pub index: PointIndex,
    pub value: u32,
}

// --- Helpers ---

/// Decode schedule time integer: 173 -> (17, 30), 80 -> (8, 0)
pub fn decode_schedule_time(encoded: u32) -> (u32, u32) {
    let hours = encoded / 10;
    let minutes = (encoded % 10) * 10;
    (hours, minutes)
}
