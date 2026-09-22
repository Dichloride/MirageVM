//! 楚门世界:append-only 的语义 fact store。
//!
//! 这里只存「语义事实」,不存任何协议字节——字节由 persona 渲染器确定性派生。
//! 事实一旦 materialize 就不可变;重复 materialize 出不同事实会被拒绝(单调性),
//! 这是跨协议自洽(nmap/curl/ssh 一致)的根本保证。

use std::collections::HashMap;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

/// 一个开放服务的语义:端口 → 服务。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceFacts {
    pub port: u16,
    pub proto: String, // "tcp" | "udp"
    pub service: String, // "ssh" | "http" | "dns" | ...
    pub version: Option<String>, // 例 "OpenSSH_9.6"
}

/// 一台主机的 persona:它「是谁」。所有协议的可见输出从这里派生。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostFacts {
    pub ip: Ipv4Addr,
    pub mac: [u8; 6],
    pub hostname: Option<String>,
    pub os: String, // "linux" 等,驱动 TCP 指纹(TTL/window)
    pub services: Vec<ServiceFacts>,
}

#[derive(Clone, Debug)]
pub struct WorldState {
    pub net: Ipv4Addr, // 子网基地址,如 10.23.0.0
    pub prefix: u8,    // 如 16
    pub gateway: Option<Ipv4Addr>,
    pub hosts: HashMap<Ipv4Addr, HostFacts>,
    pub dns: HashMap<String, Ipv4Addr>,
}

impl WorldState {
    pub fn new(net: Ipv4Addr, prefix: u8, gateway: Ipv4Addr) -> Self {
        let mut ws = Self {
            net,
            prefix,
            gateway: Some(gateway),
            hosts: HashMap::new(),
            dns: HashMap::new(),
        };
        // 网关是 seed host:它提供 DNS(udp/53)。
        let gw = HostFacts {
            ip: gateway,
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            hostname: Some("gw".into()),
            os: "linux".into(),
            services: vec![ServiceFacts {
                port: 53,
                proto: "udp".into(),
                service: "dns".into(),
                version: None,
            }],
        };
        ws.hosts.insert(gateway, gw);
        ws
    }

    /// 是否属于本子网。
    pub fn in_subnet(&self, ip: Ipv4Addr) -> bool {
        let mask = if self.prefix == 0 {
            0
        } else {
            u32::MAX << (32 - self.prefix)
        };
        (u32::from(self.net) & mask) == (u32::from(ip) & mask)
    }

    /// materialize 一台主机。已存在且事实不同 → 拒绝;相同 → 幂等。
    pub fn materialize_host(&mut self, facts: HostFacts) -> Result<(), String> {
        if let Some(existing) = self.hosts.get(&facts.ip) {
            if existing != &facts {
                return Err(format!("conflict materializing host {}", facts.ip));
            }
            return Ok(());
        }
        self.hosts.insert(facts.ip, facts);
        Ok(())
    }

    /// materialize 一条 DNS 事实。已存在且指向不同 IP → 拒绝。
    pub fn materialize_dns(&mut self, name: &str, ip: Ipv4Addr) -> Result<(), String> {
        if let Some(existing) = self.dns.get(name) {
            if *existing != ip {
                return Err(format!("conflict materializing dns {name}"));
            }
            return Ok(());
        }
        self.dns.insert(name.to_string(), ip);
        Ok(())
    }
}
