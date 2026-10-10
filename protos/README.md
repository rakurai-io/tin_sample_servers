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
| `StartExpiringPacketStream` | ReSell leader-time / post-pack (no-boost hash). Validators open when `resell` is set. |
| `StartExpiringMevPacketStream` | Mev leader-time / post-pack (boost hash). Validators open when `mev` is set. |
| `StartExpiringTpuPacketStream` | Non-leader TPU updates; optional (`UNIMPLEMENTED` ok). Mev + `enable_tpu_p2c_update` in production. |
| `StartP2cUpdateCountStream` | Per-slot send counts; optional (`UNIMPLEMENTED` ok) |

One TLS connection per endpoint UUID may multiplex Packet + Mev streams (distinct `meta.addr` hashes) plus optional TPU.

`P2cUpdateCount`: `uuid`, `slot`, `scheduler_count`, `tpu_count`, `total_count`, `p2c_tpu_enabled`.

`ExpiringPacketBatch.expiry_ms` holds the **working-bank slot** (`u32`), not a millisecond timeout. ReSell vs Mev vs TPU: same wire messages — tell them apart by which Relayer method accepted the stream. Details: [`p2c_server` README](../p2c_server/README.md#2-differentiating-resell-vs-mev-vs-tpu).
