//! Extinction d'une Google TV par le protocole Cast v2 (port 8009, TLS).
//! Une réponse de la télé est exigée avant de dire que l'extinction a été acceptée.
//! La recherche locale utilise mDNS (`_googlecast._tcp`).

use anyhow::{anyhow, Result};
use log::info;
use native_tls::TlsConnector;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const CAST_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
const CAST_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
const CAST_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";

/// La commande part avant la fin du compte, le temps que la télé s'éteigne
/// avant que l'ordinateur coupe le réseau en s'endormant.
pub const LEAD: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct CastDevice {
    pub name: String,
    pub ip: String,
    pub port: u16,
}

pub struct GoogleTvController {
    ip: String,
    port: u16,
}

impl GoogleTvController {
    pub fn new(ip: String, port: u16) -> Self {
        Self { ip, port }
    }

    pub async fn turn_off(&self) -> Result<()> {
        info!("Extinction Cast sur {}:{}", self.ip, self.port);
        self.send_standby().await
    }

    async fn send_standby(&self) -> Result<()> {
        let addr: SocketAddr = format!("{}:{}", self.ip.trim(), self.port)
            .parse()
            .map_err(|_| anyhow!("adresse invalide"))?;

        let stream = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(addr))
            .await
            .map_err(|_| anyhow!("la télé ne répond pas"))?
            .map_err(|_| anyhow!("connexion impossible"))?;

        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .map_err(|e| anyhow!("TLS : {e}"))?;
        let tokio_connector = tokio_native_tls::TlsConnector::from(connector);
        let mut tls = tokio_connector
            .connect(self.ip.trim(), stream)
            .await
            .map_err(|_| anyhow!("poignée de main refusée"))?;

        write_cast(
            &mut tls,
            CAST_CONNECTION,
            r#"{"type":"CONNECT","origin":{}}"#,
        )
        .await?;
        write_cast(&mut tls, CAST_HEARTBEAT, r#"{"type":"PING"}"#).await?;
        let _ = read_some(&mut tls, Duration::from_millis(600)).await;
        write_cast(
            &mut tls,
            CAST_RECEIVER,
            r#"{"type":"STANDBY","requestId":1}"#,
        )
        .await?;

        if !read_some(&mut tls, Duration::from_secs(4)).await {
            return Err(anyhow!("pas de confirmation"));
        }
        // Laisser la télé appliquer l'extinction avant de fermer la connexion.
        tokio::time::sleep(Duration::from_secs(3)).await;
        Ok(())
    }
}

async fn read_some(stream: &mut tokio_native_tls::TlsStream<TcpStream>, wait: Duration) -> bool {
    let mut buf = [0u8; 2048];
    match tokio::time::timeout(wait, stream.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => true,
        _ => false,
    }
}

async fn write_cast(
    stream: &mut tokio_native_tls::TlsStream<TcpStream>,
    namespace: &str,
    payload: &str,
) -> Result<()> {
    let message = encode_cast_message("sender-0", "receiver-0", namespace, payload);
    let len = (message.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&message).await?;
    stream.flush().await?;
    Ok(())
}

/// Cherche les Google TV et Chromecast du réseau pendant `timeout`.
pub fn discover(timeout: Duration) -> Result<Vec<CastDevice>> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|e| anyhow!("recherche impossible : {e}"))?;
    let receiver = daemon
        .browse("_googlecast._tcp.local.")
        .map_err(|e| anyhow!("recherche impossible : {e}"))?;

    let deadline = std::time::Instant::now() + timeout;
    let mut found = Vec::new();
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let wait = remaining.min(Duration::from_millis(250));
        if wait.is_zero() {
            break;
        }
        match receiver.recv_timeout(wait) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                if let Some(device) = device_from_info(&info) {
                    if !found.iter().any(|d: &CastDevice| d.ip == device.ip) {
                        found.push(device);
                    }
                }
            }
            Ok(_) => {}
            Err(err) => {
                if format!("{err:?}") != "Timeout" {
                    break;
                }
            }
        }
    }
    let _ = daemon.shutdown();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn device_from_info(info: &mdns_sd::ServiceInfo) -> Option<CastDevice> {
    let ip = info
        .get_addresses()
        .iter()
        .find(|addr| matches!(addr, IpAddr::V4(_)))
        .or_else(|| info.get_addresses().iter().next())
        .map(|addr| addr.to_string())?;
    let name = friendly_name(info);
    Some(CastDevice {
        name,
        ip,
        port: info.get_port(),
    })
}

fn friendly_name(info: &mdns_sd::ServiceInfo) -> String {
    if let Some(property) = info.get_properties().get("fn") {
        let value = property.val_str().trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    info.get_fullname()
        .split('.')
        .next()
        .unwrap_or("Google TV")
        .replace("\\032", " ")
}

fn encode_cast_message(source_id: &str, dest_id: &str, namespace: &str, payload_utf8: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(0x08);
    buf.push(0x00);
    write_string_field(&mut buf, 2, source_id);
    write_string_field(&mut buf, 3, dest_id);
    write_string_field(&mut buf, 4, namespace);
    buf.push(0x28);
    buf.push(0x00);
    write_string_field(&mut buf, 6, payload_utf8);
    buf
}

fn write_string_field(buf: &mut Vec<u8>, field_num: u8, val: &str) {
    buf.push((field_num << 3) | 2);
    write_varint(buf, val.len() as u64);
    buf.extend_from_slice(val.as_bytes());
}

fn write_varint(buf: &mut Vec<u8>, mut val: u64) {
    while val >= 0x80 {
        buf.push(((val & 0x7F) | 0x80) as u8);
        val >>= 7;
    }
    buf.push((val & 0x7F) as u8);
}
