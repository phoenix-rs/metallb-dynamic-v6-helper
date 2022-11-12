#![feature(ip)]

pub const IPV6_NETWORK_LENGTH: u8 = 64;
pub const IPV6_NETMASK: u128 = u128::from_be_bytes([
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0,
]);

pub mod metallb;
pub mod prefix;
