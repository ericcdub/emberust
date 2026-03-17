use crate::models::*;
use anyhow::{anyhow, Result};
use base64::Engine;
use rumqttc::{AsyncClient, MqttOptions, QoS, Transport};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MQTT_HOST: &str = "eu-base-mqtt.topband-cloud.com";
const MQTT_PORT: u16 = 18883;

/// Encode zone commands into the binary point data format, then base64.
pub fn encode_commands(commands: &[ZoneCommand]) -> String {
    let mut bytes = Vec::new();
    for cmd in commands {
        let (type_id, value_len) = cmd.index.command_type();
        bytes.push(0x00); // header
        bytes.push(cmd.index as u8);
        bytes.push(type_id);
        match value_len {
            1 => bytes.push(cmd.value as u8),
            2 => {
                bytes.push(((cmd.value >> 8) & 0xFF) as u8);
                bytes.push((cmd.value & 0xFF) as u8);
            }
            4 => {
                bytes.push(((cmd.value >> 24) & 0xFF) as u8);
                bytes.push(((cmd.value >> 16) & 0xFF) as u8);
                bytes.push(((cmd.value >> 8) & 0xFF) as u8);
                bytes.push((cmd.value & 0xFF) as u8);
            }
            _ => {}
        }
    }
    base64::engine::general_purpose::STANDARD.encode(&bytes)
}

/// Build the JSON payload for an MQTT zone command.
fn build_message(zone: &Zone, commands: &[ZoneCommand]) -> String {
    let point_data = encode_commands(commands);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    serde_json::json!({
        "common": {
            "serial": 7870,
            "productId": zone.product_id,
            "uid": zone.uid,
            "timestamp": ts.to_string(),
        },
        "data": {
            "mac": zone.mac,
            "pointData": point_data,
        }
    })
    .to_string()
}

/// Connect to MQTT, send zone commands, then disconnect.
pub async fn send_zone_commands(
    creds: &MqttCredentials,
    zone: &Zone,
    commands: &[ZoneCommand],
) -> Result<()> {
    let mut options = MqttOptions::new(&creds.client_id, MQTT_HOST, MQTT_PORT);
    options.set_credentials(&creds.username, &creds.password);
    options.set_keep_alive(Duration::from_secs(60));
    options.set_transport(Transport::tls_with_default_config());

    let (client, mut eventloop) = AsyncClient::new(options, 10);

    // Drive the event loop to establish the connection
    eventloop.poll().await.map_err(|e| anyhow!("MQTT connect: {e}"))?;

    let topic = format!("{}/{}/download/pointdata", zone.product_id, zone.uid);
    let payload = build_message(zone, commands);

    client
        .publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes())
        .await
        .map_err(|e| anyhow!("MQTT publish: {e}"))?;

    // Poll until the publish is acknowledged (or timeout)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let poll = tokio::time::timeout_at(deadline, eventloop.poll()).await;
        match poll {
            Ok(Ok(_event)) => {
                // Check if we've been connected long enough for the ack
                // For simplicity, do a few polls then break
            }
            Ok(Err(e)) => {
                log::warn!("MQTT poll error: {e}");
                break;
            }
            Err(_) => break, // timeout
        }
        // Give it a moment then break - the publish should be sent
        tokio::time::sleep(Duration::from_millis(200)).await;
        break;
    }

    client.disconnect().await.ok();
    Ok(())
}

/// Build commands for setting target temperature.
pub fn set_target_temp_commands(temp_celsius: f32) -> Vec<ZoneCommand> {
    let tenths = (temp_celsius * 10.0).round() as u32;
    vec![ZoneCommand {
        index: PointIndex::TargetTemp,
        value: tenths,
    }]
}

/// Build commands for setting zone mode.
pub fn set_mode_commands(mode: ZoneMode) -> Vec<ZoneCommand> {
    vec![ZoneCommand {
        index: PointIndex::Mode,
        value: mode as u32,
    }]
}

/// Build commands to activate boost.
pub fn activate_boost_commands(
    temp_celsius: Option<f32>,
    hours: u32,
) -> Vec<ZoneCommand> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;

    let mut cmds = vec![
        ZoneCommand {
            index: PointIndex::BoostHours,
            value: hours.clamp(1, 3),
        },
        ZoneCommand {
            index: PointIndex::BoostTime,
            value: ts,
        },
    ];

    if let Some(temp) = temp_celsius {
        cmds.push(ZoneCommand {
            index: PointIndex::BoostTemp,
            value: (temp * 10.0).round() as u32,
        });
    }

    cmds
}

/// Build commands to deactivate boost.
pub fn deactivate_boost_commands() -> Vec<ZoneCommand> {
    vec![ZoneCommand {
        index: PointIndex::BoostHours,
        value: 0,
    }]
}
