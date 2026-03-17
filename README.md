# EPH Ember Controller

A desktop application for controlling EPH Controls' Ember smart heating systems, built with Rust and [egui](https://github.com/emilk/egui). This is a port of [pyephember](https://github.com/ttroy50/pyephember) from Python to Rust.

## Features

- **Login** with your EPH Ember account credentials
- **Dashboard** showing all heating zones with live status
- **Temperature control** — adjust target temperatures per zone
- **Mode switching** — Auto, All Day, On, Off
- **Boost control** — activate/deactivate boost (1–3 hours)
- **Status indicators** — current temperature, heating state, boiler state, online/offline

## Screenshots

*Coming soon*

## Requirements

- An EPH Controls Ember heating system with a gateway connected to the internet
- An EPH Ember account (same credentials as the EPH Ember mobile app)

## Building

```bash
cargo build --release
```

The binary will be at `target/release/eph-ember.exe` (Windows) or `target/release/eph-ember` (Linux/macOS).

## Running

```bash
cargo run --release
```

Or run the built binary directly. Set `RUST_LOG=info` for debug logging:

```bash
RUST_LOG=info cargo run --release
```

## Architecture

```
src/
├── main.rs       Desktop entry point
├── lib.rs        Library root (for future Android target)
├── models.rs     Data types — zones, modes, API responses, commands
├── api.rs        HTTP client — authentication, token refresh, zone fetching
├── mqtt.rs       MQTT client — binary point data encoding, zone commands
└── app.rs        egui UI — login screen, dashboard, zone cards, backend loop
```

The app uses a channel-based architecture: the egui UI runs on the main thread and communicates with an async backend (tokio) via message passing. This keeps the UI responsive while network operations happen in the background.

### Key design decisions

- **rustls** instead of native TLS — no OpenSSL dependency, cross-platform ready
- **Core logic in `lib.rs`** — the GUI and business logic are importable as a library, allowing different entry points per platform
- **Touch-friendly UI** — larger buttons and fonts to support future Android builds via eframe's Android backend

## API

The app communicates with EPH Ember's cloud services:

- **HTTPS API** (`eu-https.topband-cloud.com`) — authentication, fetching homes and zone data
- **MQTT** (`eu-base-mqtt.topband-cloud.com:18883`) — sending control commands (temperature, mode, boost) via TLS

## Dependencies

| Crate | Purpose |
|-------|---------|
| eframe/egui | GUI framework |
| reqwest | HTTP client (rustls backend) |
| rumqttc | MQTT client |
| tokio | Async runtime |
| serde | JSON serialization |
| base64 | Point data encoding for MQTT |

## License

MIT

## Acknowledgements

- [pyephember](https://github.com/ttroy50/pyephember) — the original Python implementation and API documentation
- [egui](https://github.com/emilk/egui) — immediate mode GUI library for Rust
