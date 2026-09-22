//! 楚门 (Truman):guest 用标准网络协议观察出一个按需 materialize 的自洽互联网。
//!
//! 当前阶段:内存冒烟 demo,无需 sudo / QEMU / API key。
//! 演示「guest 第一次 ping 一个不存在的 IP → 世界 materialize 出这台主机 → ping 通」。

mod device;
mod engine;
mod guest;
mod host;
mod materialize;
mod world;

use std::net::Ipv4Addr;

use engine::Engine;
use materialize::RuleMaterializer;
use smoltcp::wire::{EthernetFrame, EthernetProtocol, Ipv4Packet};
use world::WorldState;

fn main() {
    let world = WorldState::new(
        Ipv4Addr::new(10, 23, 0, 0),
        16,
        Ipv4Addr::new(10, 23, 0, 1),
    );
    let mut engine = Engine::new(world, Box::new(RuleMaterializer));

    let target = Ipv4Addr::new(10, 23, 7, 7);
    println!("== 楚门:guest (10.23.0.5) 第一次 ping 一个不存在的 IP {target} ==\n");
    println!(
        "观察前 world.hosts: {:?}",
        engine.world.hosts.keys().collect::<Vec<_>>()
    );

    // 1) ARP:谁有 10.23.7.7?
    let out = engine.handle_frame(&guest::arp_request(target));
    println!(
        "\n[ARP] 观察后 world.hosts: {:?}  (多了 {target})",
        engine.world.hosts.keys().collect::<Vec<_>>()
    );
    println!("[ARP] 返回帧数: {}", out.len());

    // 2) ICMP echo:ping 10.23.7.7
    let mac = engine.world.hosts[&target].mac;
    let out = engine.handle_frame(&guest::icmp_echo(mac, target));
    println!("\n[ICMP] 返回帧数: {}", out.len());
    for f in &out {
        if let Ok(eth) = EthernetFrame::new_checked(f) {
            if eth.ethertype() == EthernetProtocol::Ipv4 {
                if let Ok(ip) = Ipv4Packet::new_checked(eth.payload()) {
                    println!(
                        "  ↳ 回包 src={} dst={}  (来源应是 {target})",
                        ip.src_addr(),
                        ip.dst_addr()
                    );
                }
            }
        }
    }
}
