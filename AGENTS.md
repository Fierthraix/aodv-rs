# Project Agent Notes

## Goal

Modernize this repository from its years-old Rust starter code into a working AODV implementation based on RFC 3561.

## Source Of Truth

- `rfc3561.txt` is the protocol source of truth.
- `TASKS.md` is the living implementation record and must be updated as work progresses.
- This file should let another agent resume work quickly with only `AGENTS.md`, `TASKS.md`, `rfc3561.txt`, and the codebase.

## User Requirements

- Upgrade the project to the latest Rust edition and modern dependencies.
- Use modern Tokio async I/O where networking/runtime work is needed.
- Implement AODV, not just packet parsing.
- Add tests.
- Start by planning in `TASKS.md`.
- When work completes on a task, mark it with `- [x]`.
- If the plan changes, update `TASKS.md` so it reflects reality instead of becoming stale.

## Implementation Decisions

- Target Rust edition `2024`.
- Replace deprecated Tokio 0.1 / futures 0.1 era code with Tokio 1.x style async code.
- Keep the implementation as a userspace AODV control-plane daemon plus reusable protocol engine.
- Do not assume kernel routing-table manipulation is required for this first version.
- Focus on RFC 3561 unicast control-plane behavior:
  - message encoding/decoding
  - route discovery
  - reverse/forward route maintenance
  - RREQ duplicate suppression
  - RREP generation/forwarding
  - RERR generation/processing
  - hello-based neighbor liveness
  - timer-driven expiry and maintenance
- Prefer deterministic engine logic that can be unit-tested without real sockets.
- Keep socket/runtime integration thin around the protocol engine.

## Expected Architecture

- `message` module for RFC message types and codec.
- `config` module for CLI + config file loading.
- `routing` / `engine` modules for protocol state and RFC behavior.
- `daemon` module for Tokio UDP runtime integration.
- Unit tests for packet and state-machine behavior.
- Integration or async tests for daemon-level smoke coverage where practical.

## Working Rules

- Update `TASKS.md` before and during implementation when scope changes.
- Preserve the completed history in `TASKS.md`; do not leave completed work unchecked.
- If a feature is intentionally deferred, record that explicitly in `TASKS.md`.
- Prefer small, testable protocol primitives over embedding logic directly in the network loop.

## Current Status

- The old Tokio 0.1 starter has been replaced with a Rust 2024 crate using Tokio 1.x.
- The codebase now contains:
  - a typed config loader with YAML compatibility for the old config format
  - RFC message encode/decode support for RREQ, RREP, RERR, RREP-ACK, and hello interval extensions
  - a protocol engine covering route discovery, duplicate suppression, reverse/forward route maintenance, RREP handling, RERR handling, hello tracking, and timer-driven expiry
  - a Tokio UDP daemon wrapper around the engine with real Linux UDP socket setup, interface pinning, and inbound TTL reception
  - deterministic multi-node simulation tests and real-UDP multi-node integration tests
  - a verified static build path for `x86_64-unknown-linux-musl`

## Known Limitations

- Local repair is implemented, but the repair TTL currently uses the existing route hop count plus `LOCAL_ADD_TTL` as a userspace approximation rather than deriving sender-distance from a real forwarding path.
- Buffered data-packet handling is implemented inside the protocol engine, but the daemon currently logs flush/drop actions instead of owning a full end-to-end payload queue and reinjection path.
- Kernel routing-table integration is still out of scope for this repository version.
