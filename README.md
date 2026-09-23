# DIP — Decentralized Interoperability Protocol

Routes messages between agents across multiple transports via a unified `DipEnvelope`. Adapters plug in for each network/protocol world.

**Port:** 7792 | **Security:** medium | **ARP:** custom receipts

## Architecture

```
DIP workspace
├── crates/dip-types/           # DipEnvelope, DipMessage, AdapterKind
├── crates/dip-bridge/
│   ├── src/adapters/
│   │   ├── nostr.rs            # Nostr relay (kind 30174 + ephemeral)
│   │   ├── a2a.rs              # Agent-to-Agent v1.0
│   │   ├── mcp.rs              # Model Context Protocol
│   │   ├── meshtastic.rs       # LoRa mesh (offline)
│   │   ├── libp2p.rs           # libp2p gossipsub
│   │   ├── freenet.rs          # Freenet mutable state
│   │   ├── http.rs             # generic HTTP
│   │   └── world_query.rs      # Open Reality spatial (planes/objects/path/splat)
│   └── src/router.rs           # AdapterRouter, AdapterRegistry
└── MANIFEST.toml
```

## Envelope Format

```rust
DipEnvelope {
    id:       Uuid,          // message ID
    from:     String,        // sender address
    to:       String,        // recipient address
    kind:     DipMessageKind,
    payload:  DipMessage,    // ToolCall | ToolResult | Text | Event
    ts:       u64,
}
```

## Adapters

| Adapter | Transport | Notes |
|---------|-----------|-------|
| `nostr` | WebSocket relay | agent inbox/outbox |
| `a2a` | HTTP | Agent-to-Agent v1.0 |
| `mcp` | stdio/HTTP | Model Context Protocol |
| `meshtastic` | LoRa mesh | offline-first |
| `libp2p` | P2P gossipsub | mesh federation |
| `freenet` | Freenet DHT | mutable state |
| `world_query` | HTTP | Open Reality spatial MCP (`OPEN_REALITY_URL`) |

## Environment

| Variable | Description |
|----------|-------------|
| `DIP_PORT` | HTTP server port (default: 7792) |
| `NOSTR_RELAY_URL` | Primary Nostr relay |
| `OPEN_REALITY_URL` | Open Reality service base URL (default: http://localhost:8790) |

## Quick Start

```bash
cd DIP
cargo build --release
DIP_PORT=7792 NOSTR_RELAY_URL=wss://relay.damus.io ./target/release/dip-bridge
```
