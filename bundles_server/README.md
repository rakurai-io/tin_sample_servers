# `bundles_server`

**Purpose.** Sample block-engine server so you can send bundles to Rakurai validators over the TIN.

**What it does.** Validators auth as `VALIDATOR`, call `GetBlockEngineEndpoints` for your advertised URL(s), then open subscribe streams. On each new subscription this sample builds a tip-only dummy bundle (`Keypair::new()` payer, **0.001 SOL** to a Rakurai tip account) and streams it on `SubscribeBundles`. Swap that helper for real searcher output. It does **not** serve Relayer / P2C — use [`p2c_server`](../p2c_server/) for that.

**Related:** [Repository overview](../README.md) · [Setup guide](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide) · [Tips](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/tips)

---

## 1. What it serves

| Service | Role | RPCs |
|---------|------|------|
| `auth.AuthService` | `VALIDATOR` | challenge / tokens |
| `block_engine.BlockEngineValidator` | `VALIDATOR` | `SubscribePackets`, `SubscribeBundles`, `GetBlockEngineEndpoints` |

### Discovery

**Purpose.** Advertise one or more reachable URLs without registering each region on-chain.

**What it does.** Rakurai stores **one discovery URL**. Your handler returns `global_endpoint` + `regioned_endpoints`. Validators pick the lowest-latency URL. Multi-region: implement `GetBlockEngineEndpoints` on **every** host and return the **same full list**. See [Discovery vs regioned endpoints](../README.md#2-discovery-vs-regioned-endpoints).

Edit `get_block_engine_endpoints` in `src/main.rs`. Default echoes `--public-url` into both fields; replace `regioned_endpoints` with your full list for multi-home.

---

## 2. Run

```bash
RUST_LOG=info cargo run --release -p bundles_server -- \
  --bind 0.0.0.0:10000 \
  --public-url http://<HOST_IP>:10000
```

`--bind` can be `0.0.0.0`; `--public-url` must be a host validators can reach (public URL, not loopback). Dummy bundles use `Keypair::new()`.
