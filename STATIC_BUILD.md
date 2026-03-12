# Static Build

The intended static Linux target for this project is:

- `x86_64-unknown-linux-musl`

## One-time setup

```bash
rustup target add x86_64-unknown-linux-musl
```

## Build

```bash
cargo build-static
```

Equivalent explicit command:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

## Output

The static binary is written to:

```text
target/x86_64-unknown-linux-musl/release/aodv
```

## Runtime notes

- Binding UDP port `654` still requires root or `CAP_NET_BIND_SERVICE`.
- Binding to a device with `SO_BINDTODEVICE` may require elevated privileges.
