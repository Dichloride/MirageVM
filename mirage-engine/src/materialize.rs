//! Materializer:决定「世界在此处应该是什么」的抽象。
//!
//! 只在 World State miss(语义不确定)时被调用。协议引擎绝不直接调 LLM,
//! 一切字节都由 persona 确定性派生。
//!
//! v1 提供 `RuleMaterializer`(确定性桩,无需 API key 也能跑),
//! `DeepSeekMaterializer` 之后接入。

use std::net::Ipv4Addr;

use crate::world::{HostFacts, ServiceFacts, WorldState};

pub trait Materializer: Send {
    /// 为一个从未见过的 IP 生成一台主机 persona。
    fn materialize_host(&self, ip: Ipv4Addr, world: &WorldState) -> Result<HostFacts, String>;
    /// 为一个从未见过的域名生成 DNS 事实(返回解析到的 IP)。
    fn materialize_dns(&self, name: &str, world: &WorldState) -> Result<Ipv4Addr, String>;
}

/// 确定性桩:规则化,用于测试和「无 key 也能跑」。
/// 子网内任意 IP → 一台 linux 主机(ssh/22 + http/80),MAC 由 IP 确定性派生。
pub struct RuleMaterializer;

impl RuleMaterializer {
    fn mac_for(ip: Ipv4Addr) -> [u8; 6] {
        let o = ip.octets();
        [0x02, 0x4d, o[0], o[1], o[2], o[3]]
    }

    fn hostname_for(ip: Ipv4Addr) -> String {
        let o = ip.octets();
        format!("host-{}.{}.{}.{}", o[0], o[1], o[2], o[3])
    }
}

impl Materializer for RuleMaterializer {
    fn materialize_host(&self, ip: Ipv4Addr, _world: &WorldState) -> Result<HostFacts, String> {
        Ok(HostFacts {
            ip,
            mac: Self::mac_for(ip),
            hostname: Some(Self::hostname_for(ip)),
            os: "linux".into(),
            services: vec![
                ServiceFacts {
                    port: 22,
                    proto: "tcp".into(),
                    service: "ssh".into(),
                    version: Some("OpenSSH_9.6".into()),
                },
                ServiceFacts {
                    port: 80,
                    proto: "tcp".into(),
                    service: "http".into(),
                    version: Some("nginx/1.24".into()),
                },
            ],
        })
    }

    fn materialize_dns(&self, _name: &str, world: &WorldState) -> Result<Ipv4Addr, String> {
        // 给子网里「下一个未被占用的 IP」;commit 后即固定。
        let base = u32::from(world.net);
        for i in 2..254 {
            let ip = Ipv4Addr::from(base + i);
            if !world.hosts.contains_key(&ip) {
                return Ok(ip);
            }
        }
        Err("subnet exhausted".into())
    }
}
