//! Сеть: качество канала (потери и задержка) и как к серверу подключиться снаружи.
//!
//! Качество канала меряется маленькими DNS-запросами по UDP к публичным серверам
//! (1.1.1.1 и 8.8.8.8). В отличие от TCP, у UDP нет повторной отправки, поэтому
//! каждый потерянный пакет виден как потеря. Права администратора не нужны.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::{Error, Result};

pub const PROBE_TARGETS: &[&str] = &["1.1.1.1:53", "8.8.8.8:53"];

#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    /// мс с начала эпохи
    pub ts: i64,
    /// задержка до каждой цели, мс; None — пакет потерян
    pub rtt: Vec<Option<f32>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Stats {
    pub samples: usize,
    /// потери, % (по всем целям)
    pub loss: f32,
    pub avg_rtt: f32,
    /// средний разброс задержки между соседними замерами, мс
    pub jitter: f32,
    pub max_rtt: f32,
    /// хорошо / так себе / плохо
    pub grade: String,
}

/// DNS-запрос «A example.com» с заданным id.
fn dns_query(id: u16) -> Vec<u8> {
    let mut q = vec![(id >> 8) as u8, id as u8, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for part in ["example", "com"] {
        q.push(part.len() as u8);
        q.extend_from_slice(part.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]);
    q
}

/// Один замер до одной цели. None — ответ не пришёл за `timeout`.
pub fn probe(target: &str, timeout: Duration) -> Option<f32> {
    let addr: SocketAddr = target.parse().ok()?;
    let sock = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).ok()?;
    sock.set_read_timeout(Some(timeout)).ok()?;
    let id: u16 = rand::random();
    let t0 = Instant::now();
    sock.send_to(&dns_query(id), addr).ok()?;
    let mut buf = [0u8; 512];
    loop {
        let left = timeout.checked_sub(t0.elapsed())?;
        sock.set_read_timeout(Some(left.max(Duration::from_millis(1)))).ok()?;
        match sock.recv_from(&mut buf) {
            // ответ на наш запрос (id совпадает)
            Ok((n, from)) if n >= 2 && from.ip() == addr.ip() && u16::from_be_bytes([buf[0], buf[1]]) == id => {
                return Some(t0.elapsed().as_secs_f32() * 1000.0);
            }
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

pub fn sample(targets: &[String]) -> Sample {
    // цели опрашиваются параллельно, чтобы замер занимал не больше секунды
    let handles: Vec<_> = targets
        .iter()
        .cloned()
        .map(|t| std::thread::spawn(move || probe(&t, Duration::from_millis(1000))))
        .collect();
    Sample {
        ts: chrono::Utc::now().timestamp_millis(),
        rtt: handles.into_iter().map(|h| h.join().ok().flatten()).collect(),
    }
}

pub fn stats<'a>(samples: impl IntoIterator<Item = &'a Sample>) -> Stats {
    let mut total = 0usize;
    let mut lost = 0usize;
    let mut rtts: Vec<f32> = Vec::new();
    let mut per_target_prev: Vec<Option<f32>> = Vec::new();
    let mut jit_sum = 0.0f32;
    let mut jit_n = 0usize;
    let mut n = 0usize;
    for s in samples {
        n += 1;
        if per_target_prev.len() < s.rtt.len() {
            per_target_prev.resize(s.rtt.len(), None);
        }
        for (i, r) in s.rtt.iter().enumerate() {
            total += 1;
            match r {
                Some(v) => {
                    rtts.push(*v);
                    if let Some(p) = per_target_prev[i] {
                        jit_sum += (v - p).abs();
                        jit_n += 1;
                    }
                    per_target_prev[i] = Some(*v);
                }
                None => lost += 1,
            }
        }
    }
    let loss = if total == 0 { 0.0 } else { lost as f32 * 100.0 / total as f32 };
    let avg = if rtts.is_empty() { 0.0 } else { rtts.iter().sum::<f32>() / rtts.len() as f32 };
    let jitter = if jit_n == 0 { 0.0 } else { jit_sum / jit_n as f32 };
    let grade = if total == 0 {
        String::new()
    } else if loss >= 3.0 || avg > 150.0 || jitter > 40.0 {
        "bad".into()
    } else if loss >= 0.5 || avg > 80.0 || jitter > 15.0 {
        "fair".into()
    } else {
        "good".into()
    };
    Stats { samples: n, loss, avg_rtt: avg, jitter, max_rtt: rtts.iter().cloned().fold(0.0, f32::max), grade }
}

/// Хранилище замеров (кольцевой буфер).
pub struct History {
    pub buf: VecDeque<Sample>,
    pub cap: usize,
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self { buf: VecDeque::with_capacity(cap), cap }
    }
    pub fn push(&mut self, s: Sample) {
        if self.buf.len() >= self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(s);
    }
    /// Замеры за последние `secs` секунд.
    pub fn last(&self, secs: i64) -> impl Iterator<Item = &Sample> {
        let from = chrono::Utc::now().timestamp_millis() - secs * 1000;
        self.buf.iter().filter(move |s| s.ts >= from)
    }
}

// ===================================================================== подключение

#[derive(Debug, Clone, Serialize, Default)]
pub struct ConnectInfo {
    pub local_ip: String,
    pub public_ip: String,
    pub port: u16,
    /// Порт доступен из интернета (проверка внешним сервисом). None — проверить не удалось.
    pub reachable: Option<bool>,
    /// Внешний адрес, который сообщает роутер по UPnP
    pub router_ip: String,
    pub upnp: bool,
    /// Провайдер выдаёт «серый» адрес (CGNAT) — проброс порта не поможет
    pub cgnat: bool,
    pub notes: Vec<String>,
}

/// Локальный адрес в сети (без реальной отправки пакетов).
pub fn local_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()?.ip() {
        IpAddr::V4(v) if !v.is_unspecified() => Some(v),
        _ => None,
    }
}

pub fn public_ip() -> Result<String> {
    for url in ["https://api.ipify.org", "https://ifconfig.co/ip", "https://icanhazip.com"] {
        if let Ok(t) = crate::net::get_text(url) {
            let ip = t.trim().to_string();
            if ip.parse::<IpAddr>().is_ok() {
                return Ok(ip);
            }
        }
    }
    Err(Error::Network("не удалось узнать внешний IP".into()))
}

/// Проверка порта снаружи через ifconfig.co (сервис подключается к нашему IP).
pub fn check_port(port: u16) -> Option<bool> {
    let j = crate::net::get_json(&format!("https://ifconfig.co/port/{port}")).ok()?;
    j.get("reachable").and_then(|v| v.as_bool())
}

/// «Серый» адрес: 100.64.0.0/10 (CGNAT) или частные диапазоны.
pub fn is_cgnat_or_private(ip: &str) -> bool {
    match ip.parse::<Ipv4Addr>() {
        Ok(v) => {
            let o = v.octets();
            v.is_private() || (o[0] == 100 && (64..=127).contains(&o[1]))
        }
        Err(_) => false,
    }
}

fn gateway() -> Result<igd_next::Gateway> {
    let mut opts = igd_next::SearchOptions::default();
    opts.timeout = Some(Duration::from_secs(3));
    igd_next::search_gateway(opts)
        .map_err(|e| Error::Network(format!("роутер не отвечает по UPnP ({e}). Возможно, UPnP выключен в настройках роутера")))
}

pub fn upnp_external_ip() -> Result<String> {
    Ok(gateway()?.get_external_ip().map_err(|e| Error::Network(e.to_string()))?.to_string())
}

/// Открыть TCP-порт на роутере, направив его на этот компьютер.
pub fn upnp_open(port: u16) -> Result<()> {
    let ip = local_ip().ok_or_else(|| Error::Network("не удалось определить локальный IP".into()))?;
    let gw = gateway()?;
    let local = SocketAddr::V4(SocketAddrV4::new(ip, port));
    // аренда без срока (0); некоторые роутеры её не принимают — тогда на сутки
    gw.add_port(igd_next::PortMappingProtocol::TCP, port, local, 0, "ProtoCore Minecraft")
        .or_else(|_| gw.add_port(igd_next::PortMappingProtocol::TCP, port, local, 86_400, "ProtoCore Minecraft"))
        .map_err(|e| Error::Network(format!("роутер отказал в пробросе порта: {e}")))
}

pub fn upnp_close(port: u16) -> Result<()> {
    gateway()?
        .remove_port(igd_next::PortMappingProtocol::TCP, port)
        .map_err(|e| Error::Network(e.to_string()))
}

/// Всё, что нужно, чтобы друзья подключились.
pub fn connect_info(port: u16, check_outside: bool) -> ConnectInfo {
    let mut c = ConnectInfo { port, ..Default::default() };
    c.local_ip = local_ip().map(|v| v.to_string()).unwrap_or_default();
    c.public_ip = public_ip().unwrap_or_default();
    match upnp_external_ip() {
        Ok(r) => {
            c.upnp = true;
            c.router_ip = r;
        }
        Err(e) => c.notes.push(e.to_string()),
    }
    if !c.router_ip.is_empty() && (is_cgnat_or_private(&c.router_ip) || (!c.public_ip.is_empty() && c.router_ip != c.public_ip)) {
        c.cgnat = true;
    }
    if check_outside && !c.public_ip.is_empty() {
        c.reachable = check_port(port);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_loss_and_jitter() {
        let s = |r: Vec<Option<f32>>| Sample { ts: 0, rtt: r };
        let v = vec![s(vec![Some(20.0), Some(30.0)]), s(vec![None, Some(32.0)]), s(vec![Some(24.0), Some(30.0)])];
        let st = stats(&v);
        assert!((st.loss - 16.666).abs() < 0.1);
        assert!(st.jitter > 0.0);
        assert_eq!(st.grade, "bad");
        let good = vec![s(vec![Some(20.0)]), s(vec![Some(21.0)])];
        assert_eq!(stats(&good).grade, "good");
    }

    #[test]
    fn cgnat_ranges() {
        assert!(is_cgnat_or_private("100.72.1.5"));
        assert!(is_cgnat_or_private("192.168.0.10"));
        assert!(!is_cgnat_or_private("93.184.216.34"));
    }

    /// Замер через локальный «DNS-сервер», который отвечает на каждый второй пакет.
    #[test]
    fn probe_local_server() {
        let srv = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = srv.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            let mut n = 0;
            while let Ok((len, from)) = srv.recv_from(&mut buf) {
                n += 1;
                if n % 2 == 1 {
                    let _ = srv.send_to(&buf[..len], from);
                }
            }
        });
        let t = addr.to_string();
        assert!(probe(&t, Duration::from_millis(500)).is_some());
        assert!(probe(&t, Duration::from_millis(300)).is_none());
        assert!(probe(&t, Duration::from_millis(500)).is_some());
    }
}
