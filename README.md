# aodv

[![CI](https://github.com/Fierthraix/aodv-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Fierthraix/aodv-rs/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Fierthraix/aodv-rs?display_name=tag)](https://github.com/Fierthraix/aodv-rs/releases)
[![Crates.io](https://img.shields.io/crates/v/aodv.svg)](https://crates.io/crates/aodv)
[![Downloads](https://img.shields.io/crates/d/aodv.svg)](https://crates.io/crates/aodv)
[![Docs.rs](https://docs.rs/aodv/badge.svg)](https://docs.rs/aodv)
[![License](https://img.shields.io/crates/l/aodv.svg)](Cargo.toml)
[![AUR](https://img.shields.io/aur/version/aodv)](https://aur.archlinux.org/packages/aodv)
[![AUR bin](https://img.shields.io/aur/version/aodv-bin)](https://aur.archlinux.org/packages/aodv-bin)
[![AUR git](https://img.shields.io/aur/version/aodv-git)](https://aur.archlinux.org/packages/aodv-git)

Userspace AODV control-plane implementation based on RFC 3561.

## Installation

```bash
cargo install aodv
yay -S aodv
yay -S aodv-bin
yay -S aodv-git
brew tap Fierthraix/tap
brew install --cask aodv
nix run github:Fierthraix/nur-packages#aodv
```

```powershell
scoop bucket add fierthraix https://github.com/Fierthraix/scoop-bucket
scoop install aodv
```

```text
deb/rpm/apk/tar/zip: https://github.com/Fierthraix/aodv-rs/releases
```

## Platform support

The protocol engine and UDP daemon build on Linux, macOS, and Windows. Linux
uses `SO_BINDTODEVICE` for interface pinning and receives inbound TTL metadata.
macOS uses `IP_BOUND_IF` plus the BSD TTL control-message path. Windows resolves
adapters through `GetAdaptersAddresses` and uses `IP_UNICAST_IF` for outbound
IPv4 interface selection; inbound TTL metadata is not currently exposed there.

## Routing modes

The daemon has three data-plane modes:

- `control-only`: the default. The daemon exchanges AODV control packets and
  logs route/data actions, but does not move operating-system data traffic.
- `tun-overlay`: Linux and Windows. The daemon creates or opens a TUN interface
  such as `aodv0`, reads raw IPv4 packets from it, discovers AODV routes, and
  forwards packets to the next hop over a UDP overlay socket. Windows uses the
  Wintun driver through `tun-rs`.
- `kernel-routes`: Linux-only. The daemon installs and removes Linux host routes
  for discovered AODV destinations with netlink, leaving packet forwarding to the
  kernel.

`tun-overlay` behaves like a small userspace VPN interface. It is not a physical
interface like `wlan0` or `enp2s0`; applications send normal IP traffic, Linux
routes selected destinations into the TUN device, and the daemon forwards those
packets according to the AODV route table.

`kernel-routes` is closer to classic AODV router behavior: when AODV discovers a
route, the daemon adds a route like `destination/32 via next_hop dev wlan0`.

Common CLI options have short aliases: `-l/--local-ip`, `-b/--bind-ip`,
`-B/--broadcast-ip`, `-p/--port`, `-i/--interface`, `-m/--data-plane`,
`-P/--data-port`, `-n/--tun-name`, `-t/--tun-ip`, and `-M/--tun-mtu`.

## Privileges

Full data-plane routing is not truly rootless. On Linux, creating/configuring
TUN devices and changing kernel routes require `CAP_NET_ADMIN` or root. Binding
the default AODV UDP port `654` can also require `CAP_NET_BIND_SERVICE`.
On Windows, Wintun adapter creation/configuration requires Administrator rights
and `wintun.dll` must be available next to the executable or in `PATH`.

You can grant capabilities to the built binary:

```bash
sudo setcap cap_net_admin,cap_net_bind_service+ep ./target/release/aodv
```

Or pre-create a TUN device once and let the daemon open it as a normal user:

```bash
sudo ip tuntap add dev aodv0 mode tun user "$USER"
sudo ip addr add 10.10.0.1/24 dev aodv0
sudo ip link set aodv0 up
sudo ip route add 10.10.0.0/16 dev aodv0
```

Example TUN overlay. In this mode `--local-ip` is the overlay/TUN address and
`--bind-ip` is the underlay address used for AODV UDP sockets:

```bash
aodv --interface wlan0 --bind-ip 10.0.0.1 --local-ip 10.10.0.1 --data-plane tun-overlay --tun-name aodv0
```

Linux can also let the daemon assign the TUN address when it has the required
capability:

```bash
aodv --interface wlan0 --bind-ip 10.0.0.1 --local-ip 10.10.0.1 --data-plane tun-overlay --tun-name aodv0 --tun-ip 10.10.0.1 --tun-prefix 16
```

Windows Wintun overlay requires a TUN address/prefix because there is no Linux
`ip` command equivalent in this backend:

```powershell
aodv.exe --bind-ip 10.0.0.1 --local-ip 10.10.0.1 --data-plane tun-overlay --tun-name aodv0 --tun-ip 10.10.0.1 --tun-prefix 16
```

Example kernel route mode:

```bash
aodv --interface wlan0 --local-ip 10.0.0.1 --data-plane kernel-routes
```

## RFC scope

The implementation targets RFC 3561 unicast AODV. RREQ `destination_only`,
gratuitous RREP, RERR, hello liveness, local repair, RREP-ACK, and blacklist
behavior are implemented. Multicast `join` behavior and RREP `prefix_size`
subnet routing are parsed but intentionally unsupported; routes are host routes
for now. Local repair TTL still uses the known route hop count plus
`LOCAL_ADD_TTL` because a precise sender-distance value is only available when a
future forwarding path supplies that metadata.
