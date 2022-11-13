use std::net::Ipv6Addr;

use ipnet::Ipv6Net;
use log::{debug, error, warn};
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use thiserror::Error;

#[cfg(test)]
use mockall::automock;

use super::{super::IPV6_NETWORK_LENGTH, PrefixSource, SourceError};

#[derive(Error, Debug)]
pub enum IfaceError {
    #[error("Interface `{0}` could not be found")]
    NotFound(String),
    #[error("Interface `{0}` does not have a suitable IPv6 address assigned")]
    NoIpv6Prefix(String),
    #[error("Error while looking up interfaces: `{0}`")]
    LookupError(String),
}

impl From<IfaceError> for SourceError {
    fn from(e: IfaceError) -> Self {
        SourceError { msg: e.to_string() }
    }
}

pub struct IfaceSource {
    iface_name: String,
}

impl IfaceSource {
    pub fn try_new(iface_name: String) -> Result<Box<dyn PrefixSource>, IfaceError> {
        let source = IfaceSource { iface_name };
        // Try to resolve iface addresses once, just to make sure its there
        match source.addrs() {
            Err(e) => match e {
                IfaceError::NotFound(_) => return Err(e),
                IfaceError::LookupError(_) => return Err(e),
                _ => unreachable!(),
            },
            Ok(addrs) => match source.find_v6_net(&addrs) {
                Some(_) => {}
                None => {
                    warn!(
                        "No Ipv6 address on interface {:?} while creating source, continuing",
                        source.iface_name
                    );
                }
            },
        };
        Ok(Box::new(source))
    }
    fn addrs(&self) -> Result<Vec<Addr>, IfaceError> {
        let ifs = NetworkInterface::show().map_err(|e| IfaceError::LookupError(e.to_string()))?;
        let ifaces: Vec<_> = ifs.iter().filter(|i| i.name == self.iface_name).collect();

        match ifaces.len() {
            0 => Err(IfaceError::NotFound(self.iface_name.to_string())),
            _ => Ok({
                let addrs = ifaces.iter().filter_map(|i| i.addr).collect();
                debug!(
                    "Found addresses on interface {}: {:?}",
                    self.iface_name, addrs
                );
                addrs
            }),
        }
    }

    fn find_v6_net(&self, addrs: &[Addr]) -> Option<Ipv6Net> {
        let mut v6_addrs: Vec<_> = addrs
            .iter()
            .filter_map(|a| match a {
                Addr::V4(_) => None,
                Addr::V6(v6a) => {
                    if v6a.ip.is_unicast_global()
                        && v6a.netmask.is_some()
                        && u128::from(v6a.netmask.unwrap()).count_ones()
                            == IPV6_NETWORK_LENGTH as u32
                    {
                        Some((v6a.ip, v6a.netmask.unwrap()))
                    } else {
                        debug!("Ignoring address {:?} because it is not global", v6a.ip);
                        None
                    }
                }
            })
            .collect();

        let Some((addr, mask)) = v6_addrs.pop() else {
            return None;
        };
        if !v6_addrs.is_empty() {
            warn!(
                "Multiple global IPv6 addresses in address list, selecting: {:?}",
                addr
            );
        }

        let network_part = Ipv6Addr::from(u128::from(addr) & u128::from(mask));

        match Ipv6Net::new(network_part, IPV6_NETWORK_LENGTH) {
            Ok(net) => Some(net),
            Err(e) => {
                warn!("Unable to construct Ipv6 prefix: {}", e.to_string());
                None
            }
        }
    }
}

#[cfg_attr(test, automock)]
impl PrefixSource for IfaceSource {
    fn v6_network(&self) -> Result<Ipv6Net, SourceError> {
        let addrs = match self.addrs() {
            Ok(a) => a,
            Err(e) => return Err(e.into()),
        };

        match self.find_v6_net(&addrs) {
            Some(net) => Ok(net),
            None => Err(IfaceError::NoIpv6Prefix(self.iface_name.to_string()).into()),
        }
    }
}
