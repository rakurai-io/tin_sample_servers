# `p2c_server`

**Post-pack confirmation (P2C) sample.** Implements the TIN Relayer path: validators authenticate with role `RELAYER` and open packet streams from the point of no return. Also serves `GetBlockEngineEndpoints` on `BlockEngineValidator` so validators can autoconfig / region-rank your P2C URL (same discovery RPC as bundles).

The sample parses each batch, logs transaction signatures (and slot), and is the place to plug in backrun / reply-bundle logic. It does **not** push bundles to validators — use [`bundles_server`](../bundles_server/) for the Validator/block-engine path.

**Related:** [Repository overview](../README.md) · [Using P2C](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/using_p2c)

---

## 1. What it serves

| Service | Role | RPCs |
|---------|------|------|
| `auth.AuthService` | `RELAYER` | challenge / tokens |
| `block_engine.BlockEngineValidator` | *(unauthenticated discovery)* | `GetBlockEngineEndpoints` (`SubscribePackets` / `SubscribeBundles` return `UNIMPLEMENTED`) |
| `block_engine.BlockEngineRelayer` | `RELAYER` | `StartExpiringPacketStream` (scheduler), `StartExpiringTpuPacketStream` (TPU), `StartP2cUpdateCountStream` (per-slot counts) |

### Streams (Relayer)

| RPC | Default | Disable with | Content |
|-----|---------|--------------|---------|
| `StartExpiringPacketStream` | always on | — | Scheduler / post-pack txs |
| `StartExpiringTpuPacketStream` | **on** | `--disable-tpu-packet-stream` | TPU / sigverify packets (same wire msgs) |
| `StartP2cUpdateCountStream` | **on** | `--disable-p2c-update-count` | Per-slot `uuid` / counts / `p2c_tpu_enabled` |

When disabled, the RPC returns `UNIMPLEMENTED` so newer validators keep the scheduler stream and skip the optional ones.

`expiry_ms` on each batch is the **working-bank slot** (`u32`), not a millisecond expiry.

---

## 2. Run

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001
```

| Flag | Default | Purpose |
|------|---------|---------|
| `--bind` | `0.0.0.0:10001` | gRPC listen address |
| `--public-url` | `http://127.0.0.1:10001` | Returned by `GetBlockEngineEndpoints` (must be reachable from validators) |
| `--disable-tpu-packet-stream` | off | Reject TPU packet stream |
| `--disable-p2c-update-count` | off | Reject per-slot count stream |
| `--rpc-url` / `RPC_URL` | mainnet-beta | Leader allowlist |
| `--allow-any-validator` | false | Skip allowlist (local only) |

Register the **discovery** URL with Rakurai for P2C (same idea as bundles: one URL; you advertise `global_endpoint` / `regioned_endpoints` from this process). Logs each P2C tx signature; replace that with your backrun / reply-bundle logic.

### Example: scheduler-only (no TPU, no counts)

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001 \
  --disable-tpu-packet-stream \
  --disable-p2c-update-count
```
