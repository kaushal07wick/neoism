//! mDNS/Bonjour service discovery — the "open the case and it's already
//! connected" find.
//!
//! Each Neoism instance [`advertise`](Discovery::advertise)s itself on the
//! local network under a `_neoism-sync._tcp` service carrying its
//! [`PeerId`] and TCP port. Peers [`browse`](Discovery::browse) for the
//! same service and, on resolve, get an address to hand to
//! [`TcpPeer::connect`](crate::TcpPeer::connect). The wire protocol after
//! that is identical to any other transport.
//!
//! Gated behind the `lan` feature so the dependency-light core stays lean;
//! the desktop app turns it on.

use std::net::SocketAddr;

use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::peer::PeerId;

/// The mDNS service type Neoism instances rendezvous on.
pub const SERVICE_TYPE: &str = "_neoism-sync._tcp.local.";

/// A peer found on the network, ready to dial.
#[derive(Debug, Clone)]
pub struct Discovered {
    pub peer: Option<PeerId>,
    pub addr: SocketAddr,
    pub instance: String,
}

/// Handle to the local mDNS daemon. Keep it alive for as long as you want
/// to stay advertised / keep browsing.
pub struct Discovery {
    daemon: ServiceDaemon,
}

impl Discovery {
    pub fn new() -> Result<Self, mdns_sd::Error> {
        Ok(Self {
            daemon: ServiceDaemon::new()?,
        })
    }

    /// Announce this instance. `host` is a stable label (e.g. the machine
    /// name); the address is auto-detected from the active interfaces.
    pub fn advertise(
        &self,
        peer: PeerId,
        port: u16,
        host: &str,
    ) -> Result<(), mdns_sd::Error> {
        let instance = format!("neoism-{:016x}", peer.as_u64());
        let host_name = format!("{host}.local.");
        let peer_hex = format!("{:016x}", peer.as_u64());
        let props = [("peer", peer_hex.as_str())];
        let info =
            ServiceInfo::new(SERVICE_TYPE, &instance, &host_name, "", port, &props[..])?
                .enable_addr_auto();
        self.daemon.register(info)
    }

    /// Start browsing. Drain the receiver and pass each event to
    /// [`resolve`] to extract dialable peers.
    pub fn browse(&self) -> Result<Receiver<ServiceEvent>, mdns_sd::Error> {
        self.daemon.browse(SERVICE_TYPE)
    }
}

/// Turn a resolved mDNS event into a [`Discovered`] peer, if it is one.
/// Other event kinds (search started, removed, …) yield `None`.
pub fn resolve(event: &ServiceEvent) -> Option<Discovered> {
    let info = match event {
        ServiceEvent::ServiceResolved(info) => info,
        _ => return None,
    };
    let port = info.get_port();
    // Prefer IPv4 (simplest to dial); fall back to whatever resolved.
    let scoped = info
        .get_addresses()
        .iter()
        .find(|ip| ip.is_ipv4())
        .or_else(|| info.get_addresses().iter().next())?;
    let addr = SocketAddr::new(scoped.to_ip_addr(), port);
    let peer = info
        .get_property_val_str("peer")
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        .map(PeerId);
    Some(Discovered {
        peer,
        addr,
        instance: info.get_fullname().to_string(),
    })
}
