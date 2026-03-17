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

/// Get a human-readable description for a point index
pub fn point_index_description(index: u8) -> &'static str {
    match index {
        3 => "Unknown (3)",
        4 => "ADVANCE_ACTIVE (0=Off, 1=On)",
        5 => "CURRENT_TEMP (÷10 for °C)",
        6 => "TARGET_TEMP (÷10 for °C)",
        7 => "MODE (0=Auto, 1=AllDay, 2=On, 3=Off)",
        8 => "BOOST_HOURS (0=Off, 1-3=Hours)",
        9 => "BOOST_TIME (Unix timestamp)",
        10 => "BOILER_STATE (1=Off, 2=On)",
        11 => "Unknown (11)",
        13 => "Unknown (13)",
        14 => "BOOST_TEMP (÷10 for °C)",
        15 => "CTR_15 (counter)",
        16 => "XXX_16 (unknown)",
        17 => "CTR_17 (counter)",
        18 => "CTR_18 (counter)",
        _ => "Unknown",
    }
}

/// Format a point value for display based on its index
pub fn format_point_value(index: u8, value: &str) -> String {
    if let Ok(v) = value.parse::<i64>() {
        match index {
            5 | 6 | 14 => format!("{} ({:.1}°C)", value, v as f32 / 10.0),
            7 => match v {
                0 => "0 (Auto)".to_string(),
                1 => "1 (AllDay)".to_string(),
                2 => "2 (On)".to_string(),
                3 => "3 (Off)".to_string(),
                _ => format!("{} (?)", v),
            },
            4 => match v {
                0 => "0 (Off)".to_string(),
                1 => "1 (On)".to_string(),
                _ => format!("{} (?)", v),
            },
            8 => match v {
                0 => "0 (Off)".to_string(),
                1 => "1 (1 hour)".to_string(),
                2 => "2 (2 hours)".to_string(),
                3 => "3 (3 hours)".to_string(),
                _ => format!("{} (?)", v),
            },
            10 => match v {
                1 => "1 (Off)".to_string(),
                2 => "2 (On/Heating)".to_string(),
                _ => format!("{} (?)", v),
            },
            9 => {
                // Unix timestamp - show as date/time if reasonable
                if v > 1000000000 && v < 2000000000 {
                    format!("{} (timestamp)", v)
                } else {
                    value.to_string()
                }
            }
            _ => value.to_string(),
        }
    } else {
        value.to_string()
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
        // Boost hours is 1-3 when active, 0 when inactive
        // Values > 3 indicate this field means something else on this device
        self.boost_hours().map(|h| h > 0 && h <= 3).unwrap_or(false)
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
    pub refresh_token: String,
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
    Logout,
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
    LoggedOut,
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
    pub user_id: u64,
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
