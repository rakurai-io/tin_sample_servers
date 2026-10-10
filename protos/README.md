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

| RPC | Stream | Purpose |
|-----|--------|---------|
| `StartExpiringMevPacketStream` | **Mev** | Leader-time front-run-proof pre-conf for **backrun**. Validators open when `mev` is set (MCA path). Reply bundles that include the original packet get **20% virtual priority**. |
| `StartExpiringPacketStream` | **Resell** | Leader-time front-run-proof pre-conf for **reselling**. Validators open when `resell` is set (PSA path). No 20% boost. |
| `StartExpiringTpuPacketStream` | **TPU** | Non-leader transaction stream for **backrun**; **not** front-run proof, — do not resell. |
| `StartP2cUpdateCountStream` | Count | Per-slot send counts; optional (`UNIMPLEMENTED` ok) |

One TLS connection per endpoint UUID may multiplex Mev + Resell streams (distinct `meta.addr` hashes) plus optional TPU.

`P2cUpdateCount`: `uuid`, `slot`, `scheduler_count`, `tpu_count`, `total_count`, `p2c_tpu_enabled`.

`ExpiringPacketBatch.expiry_ms` holds the **working-bank slot** (`u32`), not a millisecond timeout. Mev vs Resell vs TPU: same wire messages — tell them apart by which Relayer method accepted the stream.

- Sample definitions: [`p2c_server` README §2](../p2c_server/README.md#2-what-are-mev-resell-and-tpu-streams) · [root README](../README.md#p2c-streams-at-a-glance)
- Official docs: [Post-Pack Confirmations](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/readme) · [Setup guide §8.1](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide#81-differentiating-scheduler-vs-tpu)
