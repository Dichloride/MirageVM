//! VirtualWireDevice:smoltcp 的 Device 实现,用两个内存队列当「网线」。
//!
//! 主循环把 guest 的帧 inject 进 incoming,smoltcp 的 poll 处理;
//! smoltcp 的发送写进 outgoing,主循环取出写回 TAP。
//! 这是 Reality Boundary 在内存中的形态——真实 TAP 版只需把队列换成 fd。

use std::collections::VecDeque;

use smoltcp::phy::{ChecksumCapabilities, Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

#[derive(Debug, Default)]
pub struct VirtualWireDevice {
    pub incoming: VecDeque<Vec<u8>>,
    pub outgoing: VecDeque<Vec<u8>>,
    mtu: usize,
}

impl VirtualWireDevice {
    pub fn new(mtu: usize) -> Self {
        Self {
            incoming: VecDeque::new(),
            outgoing: VecDeque::new(),
            mtu,
        }
    }

    pub fn inject(&mut self, frame: Vec<u8>) {
        self.incoming.push_back(frame);
    }

    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing).into_iter().collect()
    }
}

pub struct WireRxToken {
    buffer: Vec<u8>,
}

pub struct WireTxToken<'a> {
    queue: &'a mut VecDeque<Vec<u8>>,
}

impl RxToken for WireRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer)
    }
}

impl<'a> TxToken for WireTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let r = f(&mut buf);
        self.queue.push_back(buf);
        r
    }
}

impl Device for VirtualWireDevice {
    type RxToken<'a> = WireRxToken;
    type TxToken<'a> = WireTxToken<'a>;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = self.mtu;
        caps.medium = Medium::Ethernet;
        caps.checksum = ChecksumCapabilities::ignored();
        caps
    }

    fn receive(&mut self, _t: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.incoming.pop_front().map(|buffer| {
            let rx = WireRxToken { buffer };
            let tx = WireTxToken {
                queue: &mut self.outgoing,
            };
            (rx, tx)
        })
    }

    fn transmit(&mut self, _t: Instant) -> Option<Self::TxToken<'_>> {
        Some(WireTxToken {
            queue: &mut self.outgoing,
        })
    }
}
