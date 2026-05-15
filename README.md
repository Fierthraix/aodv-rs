# aodv

[![CI](https://github.com/Fierthraix/aodv-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Fierthraix/aodv-rs/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/aodv.svg)](https://crates.io/crates/aodv)
[![Downloads](https://img.shields.io/crates/d/aodv.svg)](https://crates.io/crates/aodv)
[![Docs.rs](https://docs.rs/aodv/badge.svg)](https://docs.rs/aodv)
[![License](https://img.shields.io/crates/l/aodv.svg)](Cargo.toml)

Userspace AODV control-plane implementation based on RFC 3561.

## Installation

```bash
cargo install aodv
```

## Platform support

The protocol engine and UDP daemon build on Linux, macOS, and Windows. Linux
uses `SO_BINDTODEVICE` for interface pinning and receives inbound TTL metadata.
macOS uses `IP_BOUND_IF` plus the BSD TTL control-message path. Windows resolves
adapters through `GetAdaptersAddresses` and uses `IP_UNICAST_IF` for outbound
IPv4 interface selection; inbound TTL metadata is not currently exposed there.
