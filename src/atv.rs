//! Extinction d'une Google TV par la télécommande Android TV (ports 6467 puis 6466).
//! L'ordre Cast STANDBY est refusé par ces télés. Un appairage affiche un code une fois.

use anyhow::{anyhow, Result};
use native_tls::TlsConnector;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use x509_parser::prelude::*;

const PAIR_PORT: u16 = 6467;
const REMOTE_PORT: u16 = 6466;
const KEY_POWER: u64 = 26;
const KEY_WAKEUP: u64 = 224;
const CLIENT_NAME: &str = "Nemuru";

pub enum PairEvent {
    WaitingForCode,
    Paired,
    Failed(String),
}

pub struct PairSession {
    code_tx: Sender<String>,
    events: Receiver<PairEvent>,
}

pub fn is_paired() -> bool {
    marker_path().is_file() && identity_path().is_file()
}

pub fn start_pairing(ip: &str) -> Result<PairSession> {
    let ip = ip.trim().to_string();
    if ip.is_empty() {
        return Err(anyhow!("indiquez l'adresse de la télé"));
    }
    let (code_tx, code_rx) = mpsc::channel();
    let (event_tx, events) = mpsc::channel();
    std::thread::Builder::new()
        .name("appairage-tv".into())
        .spawn(move || pairing_thread(ip, code_rx, event_tx))
        .map_err(|err| anyhow!("{err}"))?;
    Ok(PairSession { code_tx, events })
}

impl PairSession {
    pub fn submit(&self, code: &str) {
        let _ = self.code_tx.send(code.trim().to_string());
    }

    pub fn key_sender(&self) -> Sender<String> {
        self.code_tx.clone()
    }

    pub fn poll(&self) -> Option<PairEvent> {
        self.events.try_recv().ok()
    }
}

#[derive(Debug)]
pub enum PowerNotice {
    Waking,
    AlreadyOn,
    WakePacket,
    Sleeping,
}

pub fn power_off(ip: &str) -> Result<()> {
    send_key(ip, KEY_POWER)
}

pub fn hardware_address(ip: &str) -> Option<String> {
    let ipv4: std::net::Ipv4Addr = ip.trim().parse().ok()?;
    let dest = u32::from_ne_bytes(ipv4.octets());
    let mut mac = [0u8; 8];
    let mut len = mac.len() as u32;
    let status = unsafe {
        windows::Win32::NetworkManagement::IpHelper::SendARP(
            dest,
            0,
            mac.as_mut_ptr() as *mut std::ffi::c_void,
            &mut len,
        )
    };
    if status != 0 || len < 6 {
        return None;
    }
    Some(format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ))
}

pub fn power_on(ip: &str, mac: &str) -> Result<PowerNotice> {
    let ip = ip.trim();
    if ip.is_empty() {
        return Err(anyhow!("indiquez l'adresse de la télé"));
    }
    if remote_open(ip) {
        if !is_paired() {
            return Err(anyhow!("appairez la télé pour la réveiller tant qu'elle répond"));
        }
        let (on, _) = panel_on(ip)?;
        if on {
            return Ok(PowerNotice::AlreadyOn);
        }
        send_key(ip, KEY_WAKEUP)?;
        return Ok(PowerNotice::Waking);
    }
    let address = if mac.trim().is_empty() {
        hardware_address(ip).unwrap_or_default()
    } else {
        mac.to_string()
    };
    wake_on_lan(ip, &address)?;
    Ok(PowerNotice::WakePacket)
}

fn remote_open(ip: &str) -> bool {
    let Ok(address) = format!("{ip}:{REMOTE_PORT}").parse() else {
        return false;
    };
    TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok()
}

fn wake_on_lan(ip: &str, mac: &str) -> Result<()> {
    let mac = parse_mac(mac).ok_or_else(|| {
        anyhow!("Nemuru n'a pas encore vu cette télé allumée")
    })?;
    let packet = magic_packet(&mac);
    let target = ip.parse::<std::net::Ipv4Addr>().ok();
    let source = target.and_then(outbound_ipv4).unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
    let socket = std::net::UdpSocket::bind((source, 0))?;
    socket.set_broadcast(true)?;
    let mut sent = false;
    for _ in 0..3 {
        sent |= socket.send_to(&packet, "255.255.255.255:9").is_ok();
        sent |= socket.send_to(&packet, "255.255.255.255:7").is_ok();
        if let Some(target) = target {
            let octets = target.octets();
            let directed = std::net::SocketAddr::from(([octets[0], octets[1], octets[2], 255], 9));
            sent |= socket.send_to(&packet, directed).is_ok();
        }
    }
    if sent {
        Ok(())
    } else {
        Err(anyhow!("le signal de réveil n'est pas parti"))
    }
}

fn outbound_ipv4(target: std::net::Ipv4Addr) -> Option<std::net::Ipv4Addr> {
    let probe = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    probe.connect((target, 9)).ok()?;
    match probe.local_addr().ok()? {
        std::net::SocketAddr::V4(address) => Some(*address.ip()),
        _ => None,
    }
}

fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let hex: String = text
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if hex.len() != 12 || hex.chars().all(|c| c == '0') || hex.chars().all(|c| c == 'f') {
        return None;
    }
    let mut mac = [0u8; 6];
    for (index, byte) in mac.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(mac)
}

fn magic_packet(mac: &[u8; 6]) -> Vec<u8> {
    let mut packet = vec![0xff; 6];
    for _ in 0..16 {
        packet.extend_from_slice(mac);
    }
    packet
}

fn panel_on(ip: &str) -> Result<(bool, u64)> {
    session(ip, None)
}

fn send_key(ip: &str, code: u64) -> Result<()> {
    session(ip, Some(code)).map(|_| ())
}

fn session(ip: &str, key: Option<u64>) -> Result<(bool, u64)> {
    if !is_paired() {
        return Err(anyhow!("la télé n'est pas appairée"));
    }
    let mut stream = connect(ip, REMOTE_PORT)?;
    let mut features = 0u64;
    let mut on = false;
    let mut sent = key.is_none();
    let mut deadline = std::time::Instant::now() + Duration::from_secs(8);
    while std::time::Instant::now() < deadline {
        let frame = match read_frame(&mut stream) {
            Ok(frame) => frame,
            Err(_) if sent => break,
            Err(err) => return Err(err),
        };
        let kind = first_field(&frame);
        if kind == 1 {
            features = nested_varint(&frame, 1).unwrap_or(features);
            write_frame(&mut stream, &configure_reply(features))?;
        } else if kind == 2 {
            write_frame(&mut stream, &active_reply(features))?;
        } else if kind == 8 {
            write_frame(&mut stream, &ping_reply(&frame))?;
        } else if kind == 40 {
            on = nested_varint(&frame, 40).unwrap_or(0) != 0;
            if let Some(code) = key {
                if !sent {
                    write_frame(&mut stream, &key_press(code))?;
                    sent = true;
                    let _ = stream.get_ref().set_read_timeout(Some(Duration::from_secs(4)));
                    deadline = std::time::Instant::now() + Duration::from_secs(4);
                }
            } else {
                return Ok((on, features));
            }
        } else if kind == 3 {
            return Err(anyhow!("la télé a refusé la télécommande"));
        }
    }
    if !sent {
        return Err(anyhow!("la télé n'a pas ouvert la télécommande"));
    }
    Ok((on, features))
}

fn pairing_thread(ip: String, code_rx: Receiver<String>, events: Sender<PairEvent>) {
    let result = (|| {
        let _ = ensure_identity()?;
        let mut stream = connect(&ip, PAIR_PORT)?;
        let server_der = stream
            .peer_certificate()?
            .ok_or_else(|| anyhow!("certificat de la télé absent"))?
            .to_der()?;
        write_frame(&mut stream, &pairing_request())?;
        let ack = read_frame(&mut stream)?;
        if polo_kind(&ack) != 11 {
            return Err(anyhow!("la télé a refusé l'appairage"));
        }
        write_frame(&mut stream, &pairing_options())?;
        let _options = read_frame(&mut stream)?;
        write_frame(&mut stream, &pairing_configuration())?;
        let confirm = read_frame(&mut stream)?;
        if polo_kind(&confirm) != 31 {
            return Err(anyhow!("la télé n'a pas affiché de code"));
        }
        log::info!("code affiché sur la télé");
        events
            .send(PairEvent::WaitingForCode)
            .map_err(|_| anyhow!("fenêtre fermée"))?;
        let code = code_rx
            .recv_timeout(Duration::from_secs(180))
            .map_err(|_| anyhow!("le code n'est pas arrivé"))?;
        let secret = pairing_secret(&server_der, &code)?;
        write_frame(&mut stream, &secret)?;
        let ack = read_frame(&mut stream)?;
        if polo_kind(&ack) != 41 {
            return Err(anyhow!("code refusé"));
        }
        std::fs::write(marker_path(), b"ok")?;
        Ok(())
    })();
    let event = match result {
        Ok(()) => PairEvent::Paired,
        Err(err) => {
            log::warn!("appairage : {err}");
            PairEvent::Failed(err.to_string())
        }
    };
    let _ = events.send(event);
}

fn connect(ip: &str, port: u16) -> Result<native_tls::TlsStream<TcpStream>> {
    let tcp = TcpStream::connect_timeout(
        &format!("{ip}:{port}")
            .parse()
            .map_err(|_| anyhow!("adresse invalide"))?,
        Duration::from_secs(5),
    )?;
    tcp.set_read_timeout(Some(Duration::from_secs(8)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(8)))?;
    let identity = native_tls::Identity::from_pkcs12(&std::fs::read(identity_path())?, "nemuru")?;
    let connector = TlsConnector::builder()
        .identity(identity)
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()?;
    Ok(connector.connect(ip, tcp)?)
}

fn ensure_identity() -> Result<()> {
    if identity_path().is_file() && key_path().is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(crate::config::AppConfig::dir())?;
    let key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).map_err(|err| anyhow!("clé : {err}"))?;
    let cert = self_signed(&key)?;
    let key_der = key
        .to_pkcs8_der()
        .map_err(|err| anyhow!("clé : {err}"))?;
    let pfx = p12::PFX::new(cert.as_slice(), key_der.as_bytes(), None, "nemuru", CLIENT_NAME)
        .ok_or_else(|| anyhow!("identité refusée"))?;
    std::fs::write(identity_path(), pfx.to_der())?;
    std::fs::write(
        key_path(),
        key.to_pkcs8_pem(LineEnding::LF)
            .map_err(|err| anyhow!("clé : {err}"))?
            .as_bytes(),
    )?;
    Ok(())
}

fn self_signed(key: &RsaPrivateKey) -> Result<Vec<u8>> {
    let modulus = key.n().to_bytes_be();
    let exponent = key.e().to_bytes_be();
    let pkcs1 = der_seq(&[&der_int(&modulus), &der_int(&exponent)]);
    let rsa_alg = der_seq(&[
        &der_oid(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01]),
        &der_null(),
    ]);
    let spki = der_seq(&[&rsa_alg, &der_bit(&pkcs1)]);
    let signature_alg = der_seq(&[
        &der_oid(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b]),
        &der_null(),
    ]);
    let name = distinguished_name();
    let validity = der_seq(&[&der_utc("251021000000Z"), &der_utc("361021000000Z")]);
    let tbs = der_seq(&[
        &der_explicit(0, &der_int(&[2])),
        &der_int(&[1]),
        &signature_alg,
        &name,
        &validity,
        &name,
        &spki,
    ]);
    let signer = SigningKey::<Sha256>::new(key.clone());
    let signature = signer.sign(&tbs);
    Ok(der_seq(&[&tbs, &signature_alg, &der_bit(&signature.to_bytes())]))
}

fn pairing_secret(server_der: &[u8], code: &str) -> Result<Vec<u8>> {
    let code = code.trim();
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!("le code a six caractères hexadécimaux"));
    }
    let key_pem = std::fs::read_to_string(key_path())?;
    let key = RsaPrivateKey::from_pkcs8_pem(&key_pem).map_err(|err| anyhow!("clé : {err}"))?;
    let (client_n, client_e) = (key.n().to_bytes_be(), key.e().to_bytes_be());
    let (server_n, server_e) = rsa_parts_from_cert(server_der)?;
    let mut hasher = Sha256::new();
    hasher.update(minimal(&client_n));
    hasher.update(exponent_bytes(&client_e));
    hasher.update(minimal(&server_n));
    hasher.update(exponent_bytes(&server_e));
    hasher.update(hex_bytes(&code[2..])?);
    let digest = hasher.finalize();
    let prefix = u8::from_str_radix(&code[..2], 16)?;
    if digest[0] != prefix {
        return Err(anyhow!("ce code ne correspond pas à la télé"));
    }
    let mut secret = Vec::new();
    field_bytes(&mut secret, 1, &digest);
    Ok(outer(40, &secret))
}

fn rsa_parts_from_cert(der: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let (_, cert) = X509Certificate::from_der(der).map_err(|err| anyhow!("certificat : {err}"))?;
    rsa_parts(cert.public_key().subject_public_key.data.as_ref())
}

fn rsa_parts(pkcs1: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let body = sequence(pkcs1)?;
    let (n, rest) = integer(body)?;
    let (e, _) = integer(rest)?;
    Ok((n, e))
}

fn distinguished_name() -> Vec<u8> {
    let attribute = der_seq(&[&der_oid(&[0x55, 0x04, 0x03]), &der_utf8(CLIENT_NAME)]);
    der_seq(&[&der_tlv(0x31, &attribute)])
}

fn der_seq(parts: &[&[u8]]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(part);
    }
    der_tlv(0x30, &body)
}

fn der_int(bytes: &[u8]) -> Vec<u8> {
    let trimmed = minimal(bytes);
    let payload: &[u8] = if trimmed.is_empty() { &[0] } else { trimmed };
    let mut body = Vec::new();
    if payload[0] & 0x80 != 0 {
        body.push(0);
    }
    body.extend_from_slice(payload);
    der_tlv(0x02, &body)
}

fn der_oid(body: &[u8]) -> Vec<u8> {
    der_tlv(0x06, body)
}

fn der_null() -> Vec<u8> {
    der_tlv(0x05, &[])
}

fn der_utf8(text: &str) -> Vec<u8> {
    der_tlv(0x0c, text.as_bytes())
}

fn der_utc(text: &str) -> Vec<u8> {
    der_tlv(0x17, text.as_bytes())
}

fn der_bit(payload: &[u8]) -> Vec<u8> {
    let mut body = vec![0];
    body.extend_from_slice(payload);
    der_tlv(0x03, &body)
}

fn der_explicit(tag: u8, inner: &[u8]) -> Vec<u8> {
    der_tlv(0xa0 | tag, inner)
}

fn der_tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = body.len();
    if len < 0x80 {
        out.push(len as u8);
    } else if len < 0x100 {
        out.extend([0x81, len as u8]);
    } else {
        out.extend([0x82, (len >> 8) as u8, len as u8]);
    }
    out.extend_from_slice(body);
    out
}

fn sequence(bytes: &[u8]) -> Result<&[u8]> {
    expect_tag(bytes, 0x30)
}

fn integer(bytes: &[u8]) -> Result<(Vec<u8>, &[u8])> {
    let (body, rest) = split_tlv(bytes, 0x02)?;
    Ok((body.to_vec(), rest))
}

fn expect_tag(bytes: &[u8], tag: u8) -> Result<&[u8]> {
    let (body, _) = split_tlv(bytes, tag)?;
    Ok(body)
}

fn split_tlv(bytes: &[u8], tag: u8) -> Result<(&[u8], &[u8])> {
    if bytes.first() != Some(&tag) {
        return Err(anyhow!("message inattendu"));
    }
    split_any(bytes)
}

fn split_any(bytes: &[u8]) -> Result<(&[u8], &[u8])> {
    if bytes.is_empty() {
        return Err(anyhow!("message trop court"));
    }
    let (len, header) = read_len(&bytes[1..])?;
    let start = 1 + header;
    let end = start + len;
    if end > bytes.len() {
        return Err(anyhow!("message trop court"));
    }
    Ok((&bytes[start..end], &bytes[end..]))
}

fn read_len(bytes: &[u8]) -> Result<(usize, usize)> {
    let first = *bytes.first().ok_or_else(|| anyhow!("message trop court"))?;
    if first < 0x80 {
        return Ok((first as usize, 1));
    }
    let n = (first & 0x7f) as usize;
    if n == 0 || n > 3 || bytes.len() < 1 + n {
        return Err(anyhow!("message trop court"));
    }
    let mut len = 0usize;
    for byte in &bytes[1..1 + n] {
        len = (len << 8) | *byte as usize;
    }
    Ok((len, 1 + n))
}

fn minimal(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|b| *b != 0).unwrap_or(0);
    if start >= bytes.len() {
        &bytes[..1.min(bytes.len())]
    } else {
        &bytes[start..]
    }
}

fn exponent_bytes(bytes: &[u8]) -> Vec<u8> {
    let hex = hex_encode(minimal(bytes)).trim_start_matches('0').to_string();
    let hex = if hex.is_empty() { "0".into() } else { hex };
    hex_bytes(&format!("0{hex}")).unwrap_or_default()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_bytes(text: &str) -> Result<Vec<u8>> {
    if text.len() % 2 != 0 {
        return Err(anyhow!("code invalide"));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| anyhow!("code invalide")))
        .collect()
}

fn pairing_request() -> Vec<u8> {
    let mut inner = Vec::new();
    field_str(&mut inner, 1, "atvremote");
    field_str(&mut inner, 2, CLIENT_NAME);
    outer(10, &inner)
}

fn pairing_options() -> Vec<u8> {
    let mut encoding = Vec::new();
    field_varint(&mut encoding, 1, 3);
    field_varint(&mut encoding, 2, 6);
    let mut options = Vec::new();
    field_bytes(&mut options, 1, &encoding);
    field_varint(&mut options, 3, 1);
    outer(20, &options)
}

fn pairing_configuration() -> Vec<u8> {
    let mut encoding = Vec::new();
    field_varint(&mut encoding, 1, 3);
    field_varint(&mut encoding, 2, 6);
    let mut configuration = Vec::new();
    field_bytes(&mut configuration, 1, &encoding);
    field_varint(&mut configuration, 2, 1);
    outer(30, &configuration)
}

fn configure_reply(features: u64) -> Vec<u8> {
    let mut info = Vec::new();
    field_varint(&mut info, 3, 1);
    field_str(&mut info, 4, "1");
    field_str(&mut info, 5, "atvremote");
    field_str(&mut info, 6, "1.0.0");
    let mut configure = Vec::new();
    field_varint(&mut configure, 1, offered(features));
    field_bytes(&mut configure, 2, &info);
    bare(1, &configure)
}

fn active_reply(features: u64) -> Vec<u8> {
    let mut active = Vec::new();
    field_varint(&mut active, 1, offered(features));
    bare(2, &active)
}

fn offered(features: u64) -> u64 {
    let known = 1 | 2 | 4 | 8 | 16 | 32 | 64 | 512;
    let offered = features & known;
    if offered == 0 { 1 | 2 | 32 | 64 | 512 } else { offered }
}

fn ping_reply(request: &[u8]) -> Vec<u8> {
    let val = nested_varint(request, 8).unwrap_or(0);
    let mut ping = Vec::new();
    field_varint(&mut ping, 1, val);
    bare(9, &ping)
}

fn key_press(code: u64) -> Vec<u8> {
    let mut key = Vec::new();
    field_varint(&mut key, 1, code);
    field_varint(&mut key, 2, 3);
    bare(10, &key)
}

fn outer(field: u32, inner: &[u8]) -> Vec<u8> {
    let mut msg = Vec::new();
    field_varint(&mut msg, 1, 2);
    field_varint(&mut msg, 2, 200);
    field_bytes(&mut msg, field, inner);
    msg
}

fn bare(field: u32, inner: &[u8]) -> Vec<u8> {
    let mut msg = Vec::new();
    field_bytes(&mut msg, field, inner);
    msg
}

fn field_varint(buf: &mut Vec<u8>, field: u32, value: u64) {
    write_varint(buf, (field as u64) << 3);
    write_varint(buf, value);
}

fn field_bytes(buf: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    write_varint(buf, ((field as u64) << 3) | 2);
    write_varint(buf, bytes.len() as u64);
    buf.extend_from_slice(bytes);
}

fn field_str(buf: &mut Vec<u8>, field: u32, text: &str) {
    field_bytes(buf, field, text.as_bytes());
}

fn write_varint(buf: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        buf.push(((value as u8) & 0x7f) | 0x80);
        value >>= 7;
    }
    buf.push((value as u8) & 0x7f);
}

fn read_varint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if index > 4 {
            return None;
        }
        value |= ((*byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((value, index + 1));
        }
        shift += 7;
    }
    None
}

fn first_field(msg: &[u8]) -> u32 {
    let Some((key, _)) = read_varint(msg) else {
        return 0;
    };
    (key >> 3) as u32
}

fn polo_kind(msg: &[u8]) -> u32 {
    let mut rest = msg;
    while !rest.is_empty() {
        let Some((key, used)) = read_varint(rest) else {
            break;
        };
        rest = &rest[used..];
        let field = (key >> 3) as u32;
        let wire = key & 7;
        if field > 2 {
            return field;
        }
        if wire == 0 {
            let Some((_, used)) = read_varint(rest) else {
                break;
            };
            rest = &rest[used..];
        } else if wire == 2 {
            let Some((len, used)) = read_varint(rest) else {
                break;
            };
            rest = rest.get(used + len as usize..).unwrap_or(&[]);
        } else {
            break;
        }
    }
    0
}

fn nested_varint(msg: &[u8], wanted: u32) -> Option<u64> {
    let mut rest = msg;
    while !rest.is_empty() {
        let (key, used) = read_varint(rest)?;
        rest = &rest[used..];
        let field = (key >> 3) as u32;
        let wire = key & 7;
        if wire == 2 {
            let (len, used) = read_varint(rest)?;
            let body = rest.get(used..used + len as usize)?;
            rest = rest.get(used + len as usize..)?;
            if field == wanted {
                let (value, _) = read_varint(body.get(1..)?)?;
                return Some(value);
            }
        } else if wire == 0 {
            let (_, used) = read_varint(rest)?;
            rest = &rest[used..];
        } else {
            return None;
        }
    }
    None
}

fn write_frame(stream: &mut impl Write, msg: &[u8]) -> Result<()> {
    let mut frame = Vec::new();
    write_varint(&mut frame, msg.len() as u64);
    frame.extend_from_slice(msg);
    stream.write_all(&frame)?;
    stream.flush()?;
    Ok(())
}

fn read_frame(stream: &mut impl Read) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    let mut one = [0u8; 1];
    let len = loop {
        stream.read_exact(&mut one)?;
        raw.push(one[0]);
        if let Some((len, _)) = read_varint(&raw) {
            if one[0] & 0x80 == 0 {
                break len as usize;
            }
        }
        if raw.len() > 5 {
            return Err(anyhow!("message trop long"));
        }
    };
    let mut msg = vec![0u8; len];
    stream.read_exact(&mut msg)?;
    Ok(msg)
}

fn identity_path() -> PathBuf {
    crate::config::AppConfig::dir().join("atv.p12")
}

fn key_path() -> PathBuf {
    crate::config::AppConfig::dir().join("atv-key.pem")
}

fn marker_path() -> PathBuf {
    crate::config::AppConfig::dir().join("atv.ok")
}

#[cfg(test)]
mod tests {
    use super::exponent_bytes;
    use rsa::signature::Verifier;
    use rsa::traits::PublicKeyParts;

    #[test]
    fn wake_packet_repeats_a_stand_in_address() {
        let mac = super::parse_mac("01-02-03-04-05-06").expect("adresse");
        let packet = super::magic_packet(&mac);
        assert_eq!(packet.len(), 102);
        assert!(packet[..6].iter().all(|byte| *byte == 0xff));
        assert_eq!(&packet[6..12], &mac);
        assert_eq!(&packet[96..102], &mac);
        assert!(super::parse_mac("00:00:00:00:00:00").is_none());
        assert!(super::parse_mac("").is_none());
    }

    #[test]
    fn exponent_matches_the_android_prefix() {
        assert_eq!(exponent_bytes(&[0x01, 0x00, 0x01]), vec![0x01, 0x00, 0x01]);
    }

    #[test]
    fn identity_certificate_carries_the_rsa_key() {
        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("clé");
        let cert = super::self_signed(&key).expect("certificat");
        let (_, parsed) = x509_parser::parse_x509_certificate(&cert).expect("analyse");
        let (modulus, exponent) =
            super::rsa_parts(parsed.public_key().subject_public_key.data.as_ref()).expect("rsa");
        assert_eq!(super::minimal(&modulus), key.n().to_bytes_be().as_slice());
        assert_eq!(super::minimal(&exponent), key.e().to_bytes_be().as_slice());
        let signature = rsa::pkcs1v15::Signature::try_from(parsed.signature_value.data.as_ref())
            .expect("signature");
        let inner = super::sequence(&cert).expect("séquence");
        let (_, rest) = super::split_any(inner).expect("contenu");
        let tbs = &inner[..inner.len() - rest.len()];
        let verifying = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(key.to_public_key());
        verifying.verify(tbs, &signature).expect("signature refusée");
    }

    #[test]
    #[ignore]
    fn wake_the_tv_named_by_the_environment() {
        let ip = std::env::var("NEMURU_TV").expect("NEMURU_TV");
        let mac = std::env::var("NEMURU_TV_MAC").unwrap_or_default();
        let notice = super::power_on(&ip, &mac).expect("réveil");
        println!("notice={notice:?}");
    }

    #[test]
    #[ignore]
    fn power_off_the_tv_named_by_nemuru_tv() {
        let ip = std::env::var("NEMURU_TV").expect("NEMURU_TV");
        let (on, features) = super::panel_on(&ip).expect("état");
        println!("avant allumée={on} fonctions={features}");
        if on {
            super::power_off(&ip).expect("extinction");
            std::thread::sleep(std::time::Duration::from_secs(5));
            let (after, _) = super::panel_on(&ip).expect("état");
            println!("après allumée={after}");
        }
        println!("cast {}", receiver_status(&ip));
    }

    fn receiver_status(ip: &str) -> String {
        let address = format!("{ip}:8009");
        let Ok(tcp) = std::net::TcpStream::connect_timeout(
            &address.parse().unwrap_or_else(|_| "127.0.0.1:9".parse().unwrap()),
            std::time::Duration::from_secs(4),
        ) else {
            return "injoignable".into();
        };
        let _ = tcp.set_read_timeout(Some(std::time::Duration::from_secs(4)));
        let _ = tcp.set_write_timeout(Some(std::time::Duration::from_secs(4)));
        let Ok(connector) = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
        else {
            return "tls".into();
        };
        let Ok(mut stream) = connector.connect(ip, tcp) else {
            return "poignée refusée".into();
        };
        let _ = cast_write(&mut stream, "urn:x-cast:com.google.cast.tp.connection", r#"{"type":"CONNECT","origin":{}}"#);
        let _ = cast_write(&mut stream, "urn:x-cast:com.google.cast.receiver", r#"{"type":"GET_STATUS","requestId":1}"#);
        let mut found = String::new();
        for _ in 0..6 {
            let Ok(frame) = cast_read(&mut stream) else {
                break;
            };
            let text = String::from_utf8_lossy(&frame);
            if let Some(start) = text.find('{') {
                let json = text[start..].trim_end_matches('\u{0}').to_string();
                if json.contains("isStandBy") {
                    return json;
                }
                found = json;
            }
        }
        if found.is_empty() { "sans statut".into() } else { found }
    }

    fn cast_write(stream: &mut impl std::io::Write, namespace: &str, payload: &str) -> std::io::Result<()> {
        let mut body = vec![0x08, 0x00];
        cast_string(&mut body, 2, "sender-0");
        cast_string(&mut body, 3, "receiver-0");
        cast_string(&mut body, 4, namespace);
        body.extend([0x28, 0x00]);
        cast_string(&mut body, 6, payload);
        stream.write_all(&(body.len() as u32).to_be_bytes())?;
        stream.write_all(&body)?;
        stream.flush()
    }

    fn cast_string(buf: &mut Vec<u8>, field: u8, value: &str) {
        buf.push((field << 3) | 2);
        let mut len = value.len();
        while len >= 0x80 {
            buf.push(((len as u8) & 0x7f) | 0x80);
            len >>= 7;
        }
        buf.push(len as u8);
        buf.extend_from_slice(value.as_bytes());
    }

    fn cast_read(stream: &mut impl std::io::Read) -> std::io::Result<Vec<u8>> {
        let mut len = [0u8; 4];
        stream.read_exact(&mut len)?;
        let mut body = vec![0u8; u32::from_be_bytes(len) as usize];
        stream.read_exact(&mut body)?;
        Ok(body)
    }
}
