# `bundles_server`

**Bundles-only sample.** Implements the TIN block-engine path: validators authenticate with role `VALIDATOR`, call `GetBlockEngineEndpoints` to get your advertised URL, then subscribe for packets and bundles you push downstream.

On each new validator subscription the sample builds a tip-only dummy bundle (`Keypair::new()` payer, 0.001 SOL to a Rakurai tip account) and streams it over `SubscribeBundles`. Swap that helper for your real searcher output when integrating.

Does **not** serve the Relayer / P2C path — use [`p2c_server`](../p2c_server/) for that.

**Related:** [Repository overview](../README.md) · [Setup guide](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide)

---

## 1. What it serves

| Service | Role | RPCs |
|---------|------|------|
| `auth.AuthService` | `VALIDATOR` | challenge / tokens |
| `block_engine.BlockEngineValidator` | `VALIDATOR` | `SubscribePackets`, `SubscribeBundles`, `GetBlockEngineEndpoints` |

`GetBlockEngineEndpoints` is discovery: Rakurai registers **this host’s URL** on-chain once. You return `global_endpoint` + `regioned_endpoints` from the handler — validators pick the lowest-latency region. You do **not** register each region separately with Rakurai. See [Discovery vs regioned endpoints](../README.md#2-discovery-vs-regioned-endpoints).

### How to add multiple regions

In `src/main.rs`, edit `get_block_engine_endpoints`. Default behavior echoes `--public-url` into both fields. Change `regioned_endpoints` to your full list (and set `global_endpoint` to your anycast / primary URL). Redeploy; no extra Rakurai registration.

---

## 2. Run

```bash
RUST_LOG=info cargo run --release -p bundles_server -- \
  --bind 0.0.0.0:10000 \
  --public-url http://<HOST_IP>:10000
```

`--bind` can be `0.0.0.0`; `--public-url` must be a host validators can connect to (not loopback / `0.0.0.0`). Dummy bundles use `Keypair::new()`.
