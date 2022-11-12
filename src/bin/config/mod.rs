use std::ffi::OsStr;
use std::time::Duration;

use clap::ValueEnum;
use clap::{arg, Parser};
use ipnet::Ipv6Net;
use log::LevelFilter;
use strum::IntoStaticStr;

// Currently available Ipv6 Prefix sources
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, ValueEnum, IntoStaticStr)]
pub enum Source {
    Iface,
}

/// Used to set the applications loglevel
// This is essentially a re-creation of log:Level. However, that enum doesn't derive ValueEnum, so we have to do it manually here
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, ValueEnum)]
pub enum Loglevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
impl From<Loglevel> for LevelFilter {
    fn from(ll: Loglevel) -> Self {
        match ll {
            Loglevel::Error => LevelFilter::Error,
            Loglevel::Warn => LevelFilter::Warn,
            Loglevel::Info => LevelFilter::Info,
            Loglevel::Debug => LevelFilter::Debug,
            Loglevel::Trace => LevelFilter::Trace,
        }
    }
}

macro_rules! env_prefix {
    () => {
        "V6HELPER_"
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Name of the IpAddressPool resource to update in k8s
    pub metallb_address_pool: String,
    /// Host range to assign to MetalLB in CIDR notation.
    /// The network part of the address is ignored.
    /// Example ::beef:0:0:0/80 + <dynamic prefix+subnet>, => 2003:abc:def:aaaa:beef:0:0:0/80
    pub metallb_host_range: Ipv6Net,

    /// Source from which to retrieve the desired IPv6 prefix from. Can be any of [`config:Source`]
    #[arg(
        value_enum,
        short = 's',
        long,
        env = concat!(env_prefix!(), "SOURCE"),
        default_value_t = Source::Iface,
        requires_if(OsStr::new(Source::Iface.into()), "iface"),

    )]
    pub source: Source,

    /// Name of the interface to check for a public prefix when using the `interface` source
    #[arg(
        long,
        env = concat!(env_prefix!(), "IFACE")
    )]
    pub iface: String,

    #[arg(
        value_enum,
        long,
        short = 'l',
        env = concat!(env_prefix!(), "LOGLEVEL"),
        default_value_t = Loglevel::Info
    )]
    pub loglevel: Loglevel,

    /// Number of seconds to wait between each run
    #[arg(
        long,
        short = 'i',
        env = concat!(env_prefix!(), "INTERVAL"),
        default_value_t = 60
    )]
    pub interval: u64,
}
