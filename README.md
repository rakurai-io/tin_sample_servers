# TIN sample servers

**Reference gRPC servers for Rakurai TIN partners.** After you register a public URL with Rakurai, opted-in validators discover and connect to your endpoint over the TIN gRPC API. These crates show the minimum setup for the two TIN paths: pushing bundles into the validator, and consuming post-pack (P2C) streams for backruns.

Working samples for local/dev and partner integration — not hardened production services. Copy the auth + service wiring, then replace dummy tip bundles and signature logging with your own searcher or backrun logic.

**Audience:** TIN partners building a block engine (bundles) and/or a post-pack (P2C) consumer.

---

## 1. Which binary?

| Crate | Serves | Use when |
|-------|--------|----------|
| [`bundles_server`](./bundles_server/) | Bundles | You authenticate as `VALIDATOR` and stream packets/bundles to validators |
| [`p2c_server`](./p2c_server/) | P2C | You authenticate as `RELAYER` and receive post-pack updates (then optionally backrun) |

| Path | Auth role | Service | Direction |
|------|-----------|---------|-----------|
| Bundles | `VALIDATOR` | `block_engine.BlockEngineValidator` | You → validator |
| Post-pack | `RELAYER` | `block_engine.BlockEngineRelayer` (+ discovery via `BlockEngineValidator.GetBlockEngineEndpoints`) | Validator → you |
| Both | — | `auth.AuthService` | Challenge → bearer token |

Run one binary if you only need that path. Same advertised URL for both paths needs **both** binaries behind one listener (or your own server that exposes Auth + Validator + Relayer together).

---

## 2. Discovery vs regioned endpoints

You do **not** manually register each regional URL with Rakurai. You register **one discovery URL**; your server advertises regions in the RPC response.

| Piece | Manual with Rakurai? | Role |
|-------|----------------------|------|
| Discovery URL | **Yes** — share once | Stored on-chain; validators dial this first |
| `global_endpoint` / `regioned_endpoints` | **No** — you return them | Validator probes regions, reconnects to lowest-latency |

### Bundles

1. Run `bundles_server` with Auth + `BlockEngineValidator`.
2. Share the discovery URL with Rakurai (e.g. `http://api.example.com:2345`).
3. Validator → discovery → `GetBlockEngineEndpoints` → your list of URLs.
4. Validator ranks `regioned_endpoints` by latency (falls back to `global_endpoint`).
5. Validator reconnects to the chosen `block_engine_url` for `SubscribePackets` / `SubscribeBundles`.

Edit **`get_block_engine_endpoints`** in [`bundles_server/src/main.rs`](./bundles_server/src/main.rs). The sample fills `regioned_endpoints` from `--public-url` only. For multi-region, put every regional URL in that `vec`. Redeploy discovery after changing it. Each listed host must run Auth + Validator gRPC.

### P2C (same discovery RPC)

`p2c_server` also implements `GetBlockEngineEndpoints` (Validator service, discovery-only; packet/bundle subscribe RPCs return `UNIMPLEMENTED`). Pass `--public-url` the same way as bundles. Validators use that response for P2C autoconfig, then open Relayer streams on the chosen host:

| Relayer RPC | Default | Opt out | What it delivers |
|-------------|---------|---------|------------------|
| `StartExpiringPacketStream` | always on | — | Point-of-no-return transactions while this validator is the leader |
| `StartExpiringTpuPacketStream` | on | `--disable-tpu-packet-stream` | Transactions seen on TPU while this validator is **not** the leader |
| `StartP2cUpdateCountStream` | on | `--disable-p2c-update-count` | Once per slot: how many transactions were successfully sent on the scheduler and TPU streams |

**Scheduler vs TPU:** both streams send the same `PacketBatchUpdate` message type. There is no `source` field on the packet — tell them apart by **which gRPC method** delivered the message. The sample logs `P2C-Update[scheduler]` / `P2C-Update[tpu]` from the handler that accepted the stream. `meta.addr` also differs (scheduler string vs validator identity signature). Details: [`p2c_server` README §2](./p2c_server/README.md#2-differentiating-scheduler-vs-tpu).

**`expiry_ms`:** despite the name, on the P2C path this field holds the validator’s **working-bank slot** (`u32`), not a millisecond timeout. The count stream carries the same slot in `P2cUpdateCount.slot`. Details: [`p2c_server` README §3](./p2c_server/README.md#3-slot-on-the-wire-expiry_ms).

---

## 3. Quick start

```bash
cargo build --release -p bundles_server

RUST_LOG=info ./target/release/bundles_server \
  --bind 0.0.0.0:10000 \
  --public-url http://<HOST_IP>:10000
```

| Flag | Meaning |
|------|---------|
| `--bind` | Listen address (`0.0.0.0` is fine) |
| `--public-url` | Returned by `GetBlockEngineEndpoints`; must be reachable from validators (not `127.0.0.1` / `0.0.0.0`) |

Share that public URL with Rakurai. Tip bundles **0.001 SOL** to a [Rakurai tip account](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/tips).

P2C:

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001
```

---

## 4. Authentication

Default allowlist: on `getLeaderSchedule` **and** `getClusterNodes` with Rakurai `client_id`.

---

## 5. Flags

| Flag | Default | Applies to | Purpose |
|------|---------|------------|---------|
| `--bind` | `0.0.0.0:10000` (`10001` for `p2c_server`) | all | gRPC listen address |
| `--public-url` | *(set for remote validators)* | `bundles_server`, `p2c_server` | URL returned by `GetBlockEngineEndpoints` |
| `--rpc-url` (`RPC_URL`) | mainnet-beta public RPC | all | Allowlist + latest blockhash |
| `--leader-refresh-secs` | `120` | all | Allowlist refresh interval |
| `--allow-any-validator` | false | all | Skip allowlist (local / lab only) |
| `--disable-tpu-packet-stream` | false | `p2c_server` | Reject the TPU packet Relayer RPC (`UNIMPLEMENTED`) |
| `--disable-p2c-update-count` | false | `p2c_server` | Reject the per-slot count Relayer RPC (`UNIMPLEMENTED`) |

---

## 6. What changed recently (P2C)

| Change | Why it matters |
|--------|----------------|
| `GetBlockEngineEndpoints` on `p2c_server` | Same discovery / region ranking as bundles; set `--public-url` |
| `StartExpiringTpuPacketStream` | Separate stream for TPU-path transactions (default on; `--disable-tpu-packet-stream` to refuse) |
| `StartP2cUpdateCountStream` | Per-slot counts of transactions sent on the scheduler and TPU streams (default on; `--disable-p2c-update-count` to refuse) |
| `expiry_ms` holds the slot | Field name is historical (Jito wire); on P2C batches the value is the working-bank slot as `u32`, not a TTL in milliseconds |
| Source in logs | Sample tags batches `scheduler` vs `tpu` from the RPC handler that accepted the stream; see also `meta.addr` |

Validators tolerate `UNIMPLEMENTED` on the optional TPU and count RPCs and keep the scheduler stream. Full notes: [`p2c_server` README](./p2c_server/README.md).

---

## 7. Layout

| Crate | Role |
|-------|------|
| [`bundles_server`](./bundles_server/) | Bundles sample |
| [`p2c_server`](./p2c_server/) | P2C sample |
| [`common`](./common/) | Auth, allowlist, dummy bundle helpers |
| [`protos`](./protos/) | `.proto` + tonic codegen |

---

## Related

- [Setup guide](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide)
- [Using P2C](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/using_p2c)
- [Tips](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/tips)
