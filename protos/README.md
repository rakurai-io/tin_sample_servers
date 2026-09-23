# `protos`

Vendored `.proto` schemas + tonic codegen. Keep wire names unchanged (validator-compatible).

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
| `StartExpiringPacketStream` | Point-of-no-return / scheduler path (required for P2C) |
| `StartExpiringTpuPacketStream` | TPU path; optional (`UNIMPLEMENTED` is fine) |
| `StartP2cUpdateCountStream` | Once per slot: counts of transactions sent on the scheduler and TPU streams; optional (`UNIMPLEMENTED` is fine) |

`P2cUpdateCount` fields: `uuid`, `slot`, `scheduler_count`, `tpu_count`, `total_count`, `p2c_tpu_enabled`.

On P2C batches, `ExpiringPacketBatch.expiry_ms` is named for Jito wire compatibility but holds the **working-bank slot** as `u32` — not a millisecond timeout. Scheduler vs TPU: same wire messages; tell them apart by **which Relayer gRPC method** accepted the stream. `meta.addr` also differs — see [`p2c_server` README](../p2c_server/README.md#2-differentiating-scheduler-vs-tpu).
