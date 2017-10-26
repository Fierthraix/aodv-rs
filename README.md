AODVD
=====

AODVD is an implementation of the Ad hoc On-Demand Distance Vector Routing protocol defined by [RFC 3561](https://www.ietf.org/rfc/rfc2561.txt) written in Go. It currently runs on any modern linux kernel.

## Dependencies

### Linux
* [ip](https://wiki.linuxfoundation.org/networking/iproute2)
* [iw](https://wireless.wiki.kernel.org/en/users/Documentation/iw) 

#### Optional
* [iptables](https://www.netfilter.org/projects/iptables/index.html) 
    - For the TopMan script
* [sysctl](https://www.kernel.org/doc/Documentation/sysctl)
    - For the start\_manet script

## TODO


## Notes

* This program overrides the operating systems routing table, and as such requires root access to run the server.
* Bytes are sent across the network in a Big Endian manner
* Currently this program only works with IPv4. To support IPv6 no fundamental changes are needed to the routing protocol. That said, many things need to be changed, including giving message types larger address spaces

## Troubleshooting

### Setting up the MANET
* To set up a MANET you can use `scripts/start_manet` to automatically join an ad-hoc network with the SSID of "aodvnet" using an IP address in the 192.168.10.1-255 range.
    - Note that on some devices the `ip link set "$INTERFACE" up` and the `iw dev "$INTERFACE" set tybe ibss` lines need to be swapped around.
* If you are using a device that does **not** have `ip` and/or `iw` the `scripts/start_manet` script contains commands using `ifconfig` and `iwconfig` commented out below their equivalent counterparts/
* If you are trying to set up an arbitrary topology for devices that are all within range you can use `scripts/topman` to assist you. Be sure to type `topman -p` on each device first to fill its ARP table.

### Troubleshooting AODV
* If AODV is setting up connections, but not forwarding data or letting you ping over one or more hops you need to make sure that packet forwarding is turned on. You can see whether or not this is enabled with `sysctl net.ipv4.ip_forward` and turn packet forwarding on with `sysctl -w net.ipv4.ip_forward=1`
* As AODV updates the kernel routing table, check that routes are correct in the operating system with `ip route`
