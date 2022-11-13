pub const IPV6_NETMASK: u128 = u128::from_be_bytes([
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0,
]);

pub mod metallb;
pub mod prefix;
