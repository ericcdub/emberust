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
fn build_message(zone: &Zone, commands: &[ZoneCommand], user_id: u64) -> String {
    let point_data = encode_commands(commands);
    let ts_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    serde_json::json!({
        "common": {
            "serial": 7870,
            "productId": zone.product_id,
            "uid": zone.uid,
            "timestamp": ts_millis.to_string(),
            "userId": user_id.to_string(),
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
    log::info!("MQTT: Connecting to {}:{}", MQTT_HOST, MQTT_PORT);
    log::info!("MQTT: client_id={}", creds.client_id);
    log::info!("MQTT: username={}", &creds.username[..creds.username.len().min(20)]);
    
    let mut options = MqttOptions::new(&creds.client_id, MQTT_HOST, MQTT_PORT);
    options.set_credentials(&creds.username, &creds.password);
    options.set_keep_alive(Duration::from_secs(60));
    options.set_transport(Transport::tls_with_default_config());

    let (client, mut eventloop) = AsyncClient::new(options, 10);

    // Drive the event loop until we're connected
    let mut connected = false;
    for _ in 0..10 {
        match eventloop.poll().await {
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(ack))) => {
                log::info!("MQTT: Connected! code={:?}", ack.code);
                if ack.code == rumqttc::ConnectReturnCode::Success {
                    connected = true;
                    break;
                } else {
                    return Err(anyhow!("MQTT connection refused: {:?}", ack.code));
                }
            }
            Ok(event) => {
                log::debug!("MQTT: event during connect: {:?}", event);
            }
            Err(e) => {
                return Err(anyhow!("MQTT connect error: {e}"));
            }
        }
    }
    
    if !connected {
        return Err(anyhow!("MQTT: Failed to receive ConnAck"));
    }

    let topic = format!("{}/{}/download/pointdata", zone.product_id, zone.uid);
    let payload = build_message(zone, commands, creds.user_id);
    
    log::info!("MQTT: Publishing to topic: {}", topic);
    log::debug!("MQTT: Payload: {}", payload);

    client
        .publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes())
        .await
        .map_err(|e| anyhow!("MQTT publish: {e}"))?;

    // Poll until the publish is acknowledged
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut published = false;
    loop {
        let poll = tokio::time::timeout_at(deadline, eventloop.poll()).await;
        match poll {
            Ok(Ok(rumqttc::Event::Incoming(rumqttc::Packet::PubAck(_)))) => {
                log::info!("MQTT: Publish acknowledged");
                published = true;
                break;
            }
            Ok(Ok(event)) => {
                log::debug!("MQTT: event: {:?}", event);
            }
            Ok(Err(e)) => {
                log::warn!("MQTT poll error: {e}");
                break;
            }
            Err(_) => {
                log::warn!("MQTT: Timeout waiting for PubAck");
                break;
            }
        }
    }

    client.disconnect().await.ok();
    
    if published {
        log::info!("MQTT: Command sent successfully");
        Ok(())
    } else {
        // Even if we didn't get a PubAck, the message may have been sent
        log::warn!("MQTT: No PubAck received, but message may have been sent");
        Ok(())
    }
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
