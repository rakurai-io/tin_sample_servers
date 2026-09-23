# `protos`

Vendored `.proto` schemas + tonic codegen. Wire names unchanged (validator-compatible).

| File | Used for |
|------|----------|
| `auth.proto` | Challenge / tokens |
| `block_engine.proto` | Validator + Relayer services (incl. P2C TPU stream, update-count, discovery) |
| `bundle.proto` | Bundle messages |
| `packet.proto` | `Packet` / `PacketBatch` |
| `shared.proto` | `Heartbeat`, `Header` |

Modules: `protos::auth`, `protos::block_engine`, …

### Relayer (P2C) RPCs in `block_engine.proto`

| RPC | Notes |
|-----|--------|
| `StartExpiringPacketStream` | Scheduler / post-pack |
| `StartExpiringTpuPacketStream` | TPU packets; optional (`UNIMPLEMENTED` ok) |
| `StartP2cUpdateCountStream` | Per-slot counts; optional (`UNIMPLEMENTED` ok) |

`P2cUpdateCount`: `uuid`, `slot`, `scheduler_count`, `tpu_count`, `total_count`, `p2c_tpu_enabled`.

On P2C batches, `ExpiringPacketBatch.expiry_ms` carries the **working-bank slot** as `u32` (not a millisecond expiry). Differentiate scheduler vs TPU by **which Relayer RPC** accepted the stream (wire msgs are identical); `meta.addr` also differs — see [`p2c_server` README](../p2c_server/README.md#2-differentiating-scheduler-vs-tpu).

