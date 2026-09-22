//! Host:一台 materialize 出的主机的「运行态」= 一个独立 smoltcp Interface。
//!
//! 每台主机拥有自己的 IP/MAC/SocketSet,因此源地址、端口、连接状态天然相互隔离。
//! 世界里的「多台主机」在 L2 上是同一个 TAP 上的多张虚拟网卡。

use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr};

use crate::device::VirtualWireDevice;
use crate::world::HostFacts;

pub struct Host {
    pub facts: HostFacts,
    pub iface: Interface,
    pub device: VirtualWireDevice,
    pub sockets: SocketSet<'static>,
}

impl Host {
    pub fn new(facts: &HostFacts, prefix: u8) -> Self {
        let mac = EthernetAddress(facts.mac);
        let ip = facts.ip; // smoltcp::wire::Ipv4Address == std::net::Ipv4Addr
        let config = Config::new(HardwareAddress::Ethernet(mac));

        let mut device = VirtualWireDevice::new(1500);
        let mut iface = Interface::new(config, &mut device, Instant::now());
        iface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::new(IpAddress::Ipv4(ip), prefix))
                .unwrap();
        });

        Self {
            facts: facts.clone(),
            iface,
            device,
            sockets: SocketSet::new(vec![]),
        }
    }

    pub fn poll(&mut self, now: Instant) {
        let Host {
            iface,
            device,
            sockets,
            ..
        } = self;
        iface.poll(now, device, sockets);
    }
}
