use std::net::Ipv6Addr;

use ipnet::Ipv6Net;
use log::{debug, error, warn};
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use thiserror::Error;

#[cfg(test)]
use mockall::automock;

use super::{PrefixSource, SourceError};

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
    network_length: u8,
}

impl IfaceSource {
    #[cfg(test)]
    pub fn test_new(iface_name: String, network_length: u8) -> IfaceSource {
        IfaceSource {
            iface_name,
            network_length,
        }
    }

    pub fn try_new(
        iface_name: String,
        network_length: u8,
    ) -> Result<Box<dyn PrefixSource>, IfaceError> {
        let source = IfaceSource {
            iface_name,
            network_length,
        };
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
                    if ip_rfc::global_v6(&v6a.ip) {
                        Some(v6a.ip)
                    } else {
                        debug!("Ignoring address {:?} because it is not global", v6a.ip);
                        None
                    }
                }
            })
            .collect();

        let Some(addr) = v6_addrs.pop() else {
            return None;
        };
        if !v6_addrs.is_empty() {
            warn!(
                "Multiple global IPv6 addresses in address list, selecting: {:?}",
                addr
            );
        }

        let netmask: u128 = !(u128::MAX >> self.network_length);
        let network_part = Ipv6Addr::from(u128::from(addr) & netmask);

        match Ipv6Net::new(network_part, self.network_length) {
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

#[cfg(test)]
mod tests {
    use std::{
        net::{Ipv4Addr, Ipv6Addr},
        str::FromStr,
    };

    use ipnet::Ipv6Net;
    use network_interface::{Addr, V4IfAddr, V6IfAddr};

    use super::IfaceSource;

    #[test]
    fn finds_correct_net() {
        let s = IfaceSource::test_new("test0".to_string(), 48);
        let r = s.find_v6_net(&[
            Addr::V6(V6IfAddr {
                ip: Ipv6Addr::from_str("fe80::bc4d:ffff:fe13:47ce").unwrap(),
                broadcast: None,
                netmask: None,
            }),
            Addr::V4(V4IfAddr {
                ip: Ipv4Addr::from_str("10.10.10.2").unwrap(),
                broadcast: None,
                netmask: None,
            }),
            Addr::V6(V6IfAddr {
                ip: Ipv6Addr::from_str("2003:ee:970c:80aa::199").unwrap(),
                broadcast: None,
                netmask: None,
            }),
        ]);
        assert!(r.is_some());
        assert_eq!(
            Ipv6Net::new(Ipv6Addr::from_str("2003:ee:970c::0").unwrap(), 48).unwrap(),
            r.unwrap()
        );
    }
}
