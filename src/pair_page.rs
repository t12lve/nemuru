//! Page locale pour saisir le code d'appairage depuis un téléphone.
//! Le téléphone parle à cet ordinateur. Le code ne quitte pas la maison.

use anyhow::{anyhow, Result};
use qrcode::QrCode;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct PairPage {
    url: String,
    modules: Vec<bool>,
    width: usize,
    codes: Receiver<String>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PairPage {
    pub fn open(toward: &str) -> Result<Self> {
        let advertise = lan_ipv4(toward).ok_or_else(|| anyhow!("aucune adresse locale"))?;
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = fresh_token();
        let url = format!("http://{advertise}:{port}/{token}");
        let code = QrCode::new(url.as_bytes()).map_err(|err| anyhow!("{err}"))?;
        let width = code.width();
        let mut modules = Vec::with_capacity(width * width);
        for y in 0..width {
            for x in 0..width {
                modules.push(code[(x, y)] == qrcode::Color::Dark);
            }
        }
        let (tx, codes) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("page-code".into())
            .spawn(move || serve(listener, token, tx, flag))
            .map_err(|err| anyhow!("{err}"))?;
        log::info!("page d'appairage prête");
        Ok(Self {
            url,
            modules,
            width,
            codes,
            stop,
            thread: Some(thread),
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn modules(&self) -> &[bool] {
        &self.modules
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn poll_code(&self) -> Option<String> {
        self.codes.try_recv().ok()
    }
}

impl Drop for PairPage {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(listener: TcpListener, token: String, codes: Sender<String>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => handle(stream, &token, &codes),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(_) => break,
        }
    }
}

fn handle(mut stream: TcpStream, token: &str, codes: &Sender<String>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(4)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(4)));
    let Some((start, header, body)) = read_http(&mut stream) else {
        return;
    };
    let path = request_target(&start).unwrap_or("/");
    if !same_token(path, token) {
        let _ = respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", "Introuvable");
        return;
    }
    let method = start.split_whitespace().next().unwrap_or("");
    if method == "GET" {
        let _ = respond(&mut stream, "200 OK", "text/html; charset=utf-8", &page(None));
        return;
    }
    if method != "POST" {
        let _ = respond(&mut stream, "405 Method Not Allowed", "text/plain; charset=utf-8", "Méthode refusée");
        return;
    }
    let length = content_length(&header).unwrap_or(0).min(512);
    let text = String::from_utf8_lossy(&body[..body.len().min(length)]);
    match code_from_body(&text) {
        Some(code) => {
            let _ = codes.send(code);
            let _ = respond(&mut stream, "200 OK", "text/html; charset=utf-8", &done_page());
        }
        None => {
            let _ = respond(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                &page(Some("Le code a six caractères.")),
            );
        }
    }
}

fn read_http(stream: &mut TcpStream) -> Option<(String, String, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let header_end = loop {
        if buf.len() > 8192 {
            return None;
        }
        match stream.read(&mut tmp) {
            Ok(0) => return None,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = find_sep(&buf) {
                    break pos;
                }
            }
            Err(_) => return None,
        }
    };
    let header = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let start = header.lines().next()?.to_string();
    let mut body = buf[header_end + 4..].to_vec();
    let length = content_length(&header).unwrap_or(0).min(512);
    while body.len() < length {
        match stream.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
        }
    }
    body.truncate(length);
    Some((start, header, body))
}

fn find_sep(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn request_target(start: &str) -> Option<&str> {
    let target = start.split_whitespace().nth(1)?;
    Some(target.split('?').next().unwrap_or(target))
}

fn content_length(header: &str) -> Option<usize> {
    header.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

fn same_token(path: &str, token: &str) -> bool {
    let Some(found) = path.strip_prefix('/') else {
        return false;
    };
    if found.len() != token.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in found.bytes().zip(token.bytes()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn code_from_body(body: &str) -> Option<String> {
    for pair in body.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == "code" {
            let cleaned: String = value
                .chars()
                .filter(|ch| ch.is_ascii_hexdigit())
                .map(|ch| ch.to_ascii_uppercase())
                .collect();
            if cleaned.len() == 6 {
                return Some(cleaned);
            }
        }
    }
    None
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) -> std::io::Result<()> {
    let bytes = body.as_bytes();
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\r\n",
        bytes.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(bytes)?;
    Ok(())
}

fn page(notice: Option<&str>) -> String {
    let notice = notice.unwrap_or("");
    format!(
        r#"<!DOCTYPE html>
<html lang="fr">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Nemuru</title>
<style>
body {{ margin: 0; min-height: 100vh; background: #161412; color: #ece6dc; font: 18px "Segoe UI", sans-serif; display: flex; align-items: center; justify-content: center; }}
main {{ width: min(420px, 92vw); }}
h1 {{ font-weight: 400; font-size: 28px; margin: 0 0 12px; }}
p {{ color: #b0a698; line-height: 1.4; }}
input {{ width: 100%; box-sizing: border-box; background: #241f1b; color: #ece6dc; border: 0; font-size: 32px; letter-spacing: 0.28em; padding: 16px; }}
button {{ margin-top: 16px; width: 100%; background: #c49a5c; color: #161412; border: 0; font-size: 18px; padding: 14px; }}
</style>
<main>
<h1>Nemuru</h1>
<p>Le code affiché sur la télé.</p>
<p>{notice}</p>
<form method="post">
<input name="code" maxlength="6" autocapitalize="characters" autocomplete="off" spellcheck="false" autofocus>
<button type="submit">Envoyer</button>
</form>
</main>
<script>
const map = {{'&':'1','é':'2','"':'3',"'":'4','(':'5','-':'6','è':'7','_':'8','ç':'9','à':'0'}};
const field = document.querySelector('input');
field.addEventListener('input', () => {{
  field.value = Array.from(field.value).map(ch => map[ch] || ch).join('').replace(/[^0-9A-Fa-f]/g, '').slice(0, 6).toUpperCase();
}});
</script>
"#
    )
}

fn done_page() -> String {
    r#"<!DOCTYPE html>
<html lang="fr">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Nemuru</title>
<style>
body { margin: 0; min-height: 100vh; background: #161412; color: #ece6dc; font: 18px "Segoe UI", sans-serif; display: flex; align-items: center; justify-content: center; }
main { width: min(420px, 92vw); }
h1 { font-weight: 400; font-size: 28px; }
p { color: #b0a698; line-height: 1.4; }
</style>
<main>
<h1>Reçu</h1>
<p>Nemuru a le code. Vous pouvez poser le téléphone.</p>
</main>
"#
    .to_string()
}

fn fresh_token() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn lan_ipv4(toward: &str) -> Option<Ipv4Addr> {
    let peer = toward.trim().parse::<Ipv4Addr>().ok()?;
    let probe = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    probe.connect((peer, 9)).ok()?;
    match probe.local_addr().ok()? {
        std::net::SocketAddr::V4(address) => {
            let ip = *address.ip();
            (ip != Ipv4Addr::UNSPECIFIED).then_some(ip)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phone_post_delivers_the_code() {
        let page = PairPage::open("127.0.0.1").expect("page");
        let url = page.url().to_string();
        let body = "code=ab12ef";
        let mut stream = TcpStream::connect(url.trim_start_matches("http://").split('/').next().unwrap()).unwrap();
        let token_path = url.split('/').nth(3).unwrap();
        let request = format!(
            "POST /{token_path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("Reçu"));
        assert_eq!(page.poll_code().as_deref(), Some("AB12EF"));
    }

    #[test]
    fn a_wrong_path_stays_closed() {
        assert!(!same_token("/autre", "jeton"));
        assert!(same_token("/jeton", "jeton"));
        assert_eq!(code_from_body("code=abc"), None);
        assert_eq!(code_from_body("code=abc123").as_deref(), Some("ABC123"));
    }
}
