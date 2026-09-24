# `protos`

**Purpose.** Wire schemas the sample servers compile against so message and RPC names match what Rakurai validators expect.

**What it does.** Vendors `.proto` files and generates tonic stubs. Keep names unchanged — renaming breaks the validator connection.

| File | Purpose |
|------|---------|
| `auth.proto` | Challenge / tokens (`AuthService`) |
| `block_engine.proto` | Discovery + Validator/Relayer (P2C streams, counts) |
| `bundle.proto` | Bundle messages |
| `packet.proto` | `Packet` / `PacketBatch` |
| `shared.proto` | `Heartbeat`, `Header` |

Modules: `protos::auth`, `protos::block_engine`, …

### Relayer (P2C) RPCs

| RPC | Purpose |
|-----|---------|
| `StartExpiringPacketStream` | Leader-time / post-pack updates (required) |
| `StartExpiringTpuPacketStream` | Non-leader TPU updates; optional (`UNIMPLEMENTED` ok). In production, **MCA** path (**PSA included**) |
| `StartP2cUpdateCountStream` | Per-slot send counts; optional (`UNIMPLEMENTED` ok) |

`P2cUpdateCount`: `uuid`, `slot`, `scheduler_count`, `tpu_count`, `total_count`, `p2c_tpu_enabled`.

`ExpiringPacketBatch.expiry_ms` holds the **working-bank slot** (`u32`), not a millisecond timeout. Scheduler vs TPU: same wire messages — tell them apart by which Relayer method accepted the stream. Details: [`p2c_server` README](../p2c_server/README.md#2-differentiating-scheduler-vs-tpu).
