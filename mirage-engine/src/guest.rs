//! 构造 guest 侧帧的辅助工具(测试/演示用),扮演「楚门」的客户机。
//!
//! 手写字节(ARP 请求 42 字节、ICMP echo),避免依赖 smoltcp 的 Repr 构造——
//! 那些 Repr 是 `#[non_exhaustive]`,外部 crate 无法用字面量构造。

use std::net::Ipv4Addr;

pub const GUEST_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
pub const GUEST_IP: Ipv4Addr = Ipv4Addr::new(10, 23, 0, 5);

const ETHERTYPE_ARP: [u8; 2] = [0x08, 0x06];
const ETHERTYPE_IPV4: [u8; 2] = [0x08, 0x00];

fn eth(dst: [u8; 6], ethertype: [u8; 2], payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(14 + payload.len());
    v.extend_from_slice(&dst);
    v.extend_from_slice(&GUEST_MAC);
    v.extend_from_slice(&ethertype);
    v.extend_from_slice(payload);
    v
}

/// ARP request:「谁有 target_ip?」
pub fn arp_request(target: Ipv4Addr) -> Vec<u8> {
    let mut arp = Vec::with_capacity(28);
    arp.extend_from_slice(&[0x00, 0x01]); // htype = ethernet
    arp.extend_from_slice(&[0x08, 0x00]); // ptype = ipv4
    arp.push(6); // hlen
    arp.push(4); // plen
    arp.extend_from_slice(&[0x00, 0x01]); // op = request
    arp.extend_from_slice(&GUEST_MAC); // sender hw
    arp.extend_from_slice(&GUEST_IP.octets()); // sender ip
    arp.extend_from_slice(&[0u8; 6]); // target hw (zero)
    arp.extend_from_slice(&target.octets()); // target ip
    eth([0xff; 6], ETHERTYPE_ARP, &arp)
}

/// ICMP echo request(ping)。checksum 填 0——宿主的 device 声明 ignored,smoltcp 不校验。
pub fn icmp_echo(dst_mac: [u8; 6], dst_ip: Ipv4Addr) -> Vec<u8> {
    let mut icmp = Vec::new();
    icmp.push(8); // type = echo request
    icmp.push(0); // code
    icmp.extend_from_slice(&[0x00, 0x00]); // checksum
    icmp.extend_from_slice(&[0x12, 0x34]); // ident
    icmp.extend_from_slice(&[0x00, 0x01]); // seq
    icmp.extend_from_slice(b"ping-from-truman");

    let mut ip = Vec::with_capacity(20);
    ip.push(0x45); // ver=4, IHL=5
    ip.push(0x00); // DSCP/ECN
    ip.extend_from_slice(&((20 + icmp.len()) as u16).to_be_bytes()); // total len
    ip.extend_from_slice(&[0x00, 0x00]); // ident
    ip.extend_from_slice(&[0x40, 0x00]); // flags/offset (DF)
    ip.push(64); // TTL
    ip.push(1); // protocol = ICMP
    ip.extend_from_slice(&[0x00, 0x00]); // header checksum
    ip.extend_from_slice(&GUEST_IP.octets()); // src
    ip.extend_from_slice(&dst_ip.octets()); // dst

    ip.extend_from_slice(&icmp);
    eth(dst_mac, ETHERTYPE_IPV4, &ip)
}
