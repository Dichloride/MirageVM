//! Engine:楚门世界的调度核心 = Reality Boundary。
//!
//! 读帧 → 解析语义边界(ARP 目标 / IP 目标)→ World State miss 就 materialize
//! → 分发到对应 Host → 收集出帧。
//!
//! 确定性协议机制由 smoltcp 完成,只有「这个 IP 是谁」这种语义不确定处才触碰 materializer。

use std::collections::HashMap;
use std::net::Ipv4Addr;

use smoltcp::time::Instant;
use smoltcp::wire::{ArpPacket, ArpRepr, EthernetFrame, EthernetProtocol, Ipv4Packet};

use crate::host::Host;
use crate::materialize::Materializer;
use crate::world::{HostFacts, WorldState};

pub struct Engine {
    pub world: WorldState,
    pub materializer: Box<dyn Materializer>,
    pub hosts: HashMap<Ipv4Addr, Host>,
}

impl Engine {
    pub fn new(world: WorldState, materializer: Box<dyn Materializer>) -> Self {
        // 预建 seed hosts(网关等)。
        let seed: Vec<HostFacts> = world.hosts.values().cloned().collect();
        let mut hosts = HashMap::new();
        for f in seed {
            hosts.insert(f.ip, Host::new(&f, world.prefix));
        }
        Self {
            world,
            materializer,
            hosts,
        }
    }

    /// 确保某 IP 有一台运行中的 Host;没有就 materialize(唯一触发 LLM 的地方)。
    pub fn ensure_host(&mut self, ip: Ipv4Addr) -> Result<(), String> {
        if self.hosts.contains_key(&ip) {
            return Ok(());
        }
        let facts = match self.world.hosts.get(&ip) {
            Some(f) => f.clone(),
            None => {
                let f = self.materializer.materialize_host(ip, &self.world)?;
                self.world.materialize_host(f.clone())?;
                f
            }
        };
        let host = Host::new(&facts, self.world.prefix);
        self.hosts.insert(ip, host);
        Ok(())
    }

    /// 处理一帧,返回要发回 guest 的帧集合。
    pub fn handle_frame(&mut self, frame: &[u8]) -> Vec<Vec<u8>> {
        let eth = match EthernetFrame::new_checked(frame) {
            Ok(e) => e,
            Err(_) => return vec![],
        };

        match eth.ethertype() {
            EthernetProtocol::Arp => {
                let target = match ArpPacket::new_checked(eth.payload())
                    .ok()
                    .and_then(|arp| ArpRepr::parse(&arp).ok())
                {
                    Some(ArpRepr::EthernetIpv4 {
                        target_protocol_addr,
                        ..
                    }) => target_protocol_addr,
                    _ => return vec![],
                };
                self.deliver(target, frame)
            }
            EthernetProtocol::Ipv4 => {
                let dst = match Ipv4Packet::new_checked(eth.payload()) {
                    Ok(ip) => ip.dst_addr(),
                    Err(_) => return vec![],
                };
                self.deliver(dst, frame)
            }
            _ => vec![],
        }
    }

    fn deliver(&mut self, ip: Ipv4Addr, frame: &[u8]) -> Vec<Vec<u8>> {
        if !self.world.in_subnet(ip) {
            return vec![];
        }
        if let Err(e) = self.ensure_host(ip) {
            eprintln!("materialize {ip}: {e}");
            return vec![];
        }
        let host = match self.hosts.get_mut(&ip) {
            Some(h) => h,
            None => return vec![],
        };
        host.device.inject(frame.to_vec());
        host.poll(Instant::now());
        host.device.drain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smoltcp::phy::ChecksumCapabilities;
    use smoltcp::wire::{ArpOperation, Icmpv4Packet, Icmpv4Repr};

    use crate::guest;
    use crate::materialize::RuleMaterializer;

    fn engine() -> Engine {
        let world = WorldState::new(
            Ipv4Addr::new(10, 23, 0, 0),
            16,
            Ipv4Addr::new(10, 23, 0, 1),
        );
        Engine::new(world, Box::new(RuleMaterializer))
    }

    #[test]
    fn world_materialize_is_monotonic() {
        let mut w = WorldState::new(
            Ipv4Addr::new(10, 23, 0, 0),
            16,
            Ipv4Addr::new(10, 23, 0, 1),
        );
        let ip = Ipv4Addr::new(10, 23, 7, 7);
        let f = RuleMaterializer.materialize_host(ip, &w).unwrap();
        assert!(w.materialize_host(f.clone()).is_ok());
        assert!(w.materialize_host(f.clone()).is_ok(), "幂等");
        let mut g = f.clone();
        g.os = "windows".into();
        assert!(w.materialize_host(g).is_err(), "冲突应被拒绝");
    }

    #[test]
    fn arp_for_unknown_ip_materializes_and_replies() {
        let mut e = engine();
        let target = Ipv4Addr::new(10, 23, 7, 7);
        assert!(!e.world.hosts.contains_key(&target));

        let out = e.handle_frame(&guest::arp_request(target));

        assert!(e.world.hosts.contains_key(&target), "host materialized");
        assert!(!out.is_empty(), "ARP reply emitted");
        let eth = EthernetFrame::new_checked(&out[0]).unwrap();
        let arp = ArpPacket::new_checked(eth.payload()).unwrap();
        if let ArpRepr::EthernetIpv4 {
            operation,
            source_protocol_addr,
            target_protocol_addr,
            ..
        } = ArpRepr::parse(&arp).unwrap()
        {
            assert_eq!(operation, ArpOperation::Reply);
            assert_eq!(source_protocol_addr, target);
            assert_eq!(target_protocol_addr, guest::GUEST_IP);
        } else {
            panic!("expected ARP reply");
        }
    }

    #[test]
    fn ping_replies_from_materialized_source() {
        let mut e = engine();
        let target = Ipv4Addr::new(10, 23, 7, 7);
        e.handle_frame(&guest::arp_request(target));
        let mac = e.world.hosts[&target].mac;

        let out = e.handle_frame(&guest::icmp_echo(mac, target));

        assert!(!out.is_empty(), "echo reply emitted");
        let eth = EthernetFrame::new_checked(&out[0]).unwrap();
        let ip = Ipv4Packet::new_checked(eth.payload()).unwrap();
        assert_eq!(ip.src_addr(), target, "reply source = host IP");
        assert_eq!(ip.dst_addr(), guest::GUEST_IP);
        let icmp = Icmpv4Packet::new_checked(ip.payload()).unwrap();
        let repr = Icmpv4Repr::parse(&icmp, &ChecksumCapabilities::ignored()).unwrap();
        assert!(matches!(repr, Icmpv4Repr::EchoReply { .. }));
    }
}
