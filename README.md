# TIN sample servers

**Purpose.** Reference gRPC servers so TIN partners can see how to connect to Rakurai validators: send bundles inbound, and receive post-pack (P2C) updates outbound.

**What it does.** After you register a public URL with Rakurai, opted-in validators discover and dial your endpoint. This repo ships two sample binaries that implement the minimum auth + discovery + stream surface for each path. They are for local/dev and integration — not production. Copy the wiring; replace dummy tips / logging with your own logic.

**Audience:** TIN partners building a block engine (bundles) and/or a P2C consumer.

---

## 1. Which binary?

| Crate | Purpose | What it does |
|-------|---------|--------------|
| [`bundles_server`](./bundles_server/) | Send orderflow to validators | Auth as `VALIDATOR`; push packets/bundles; advertise regions via discovery |
| [`p2c_server`](./p2c_server/) | Receive post-pack updates | Auth as `RELAYER`; accept scheduler / TPU / count streams; advertise regions via discovery |

| Path | Auth role | Service | Direction |
|------|-----------|---------|-----------|
| Bundles | `VALIDATOR` | `block_engine.BlockEngineValidator` | You → validator |
| Post-pack | `RELAYER` | `block_engine.BlockEngineRelayer` (+ `GetBlockEngineEndpoints`) | Validator → you |
| Both | — | `auth.AuthService` | Challenge → bearer token |

Run one binary if you only need that path. One advertised URL for both paths needs Auth + Validator + Relayer on the same listener (or both samples behind one front door).

**P2C access (production):** prepaid **[PSA](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/psa)** for pre-conf / reselling (leader-time only), **or** **[MCA](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/mca)** for backrun / MevShare (**PSA is included** in MCA; leader-time + TPU). The sample can expose both Relayer streams for testing either path.

---

## 2. Discovery vs regioned endpoints

**Purpose.** Let validators find the closest copy of your service without registering every region on-chain.

**What it does.** You share **one discovery URL** with Rakurai. Every host that may receive a connection implements `GetBlockEngineEndpoints` and returns the same full list (`global_endpoint` + `regioned_endpoints`). Validators probe latency and reconnect to the best URL.

| Piece | Manual with Rakurai? | Role |
|-------|----------------------|------|
| Discovery URL | **Yes** — share once | On-chain seed URL |
| `global_endpoint` / `regioned_endpoints` | **No** — returned by RPC | Full list of public URLs |

### Bundles

1. Run `bundles_server` (Auth + `BlockEngineValidator`).
2. Share the discovery URL with Rakurai.
3. Validator calls `GetBlockEngineEndpoints` → your URL list.
4. Validator picks the best region → `SubscribePackets` / `SubscribeBundles`.

Edit `get_block_engine_endpoints` in [`bundles_server/src/main.rs`](./bundles_server/src/main.rs). Multi-region: put every regional URL in `regioned_endpoints` on **every** server. See sample code.

### P2C (same discovery RPC)

`p2c_server` implements `GetBlockEngineEndpoints` (subscribe RPCs stay `UNIMPLEMENTED`). Same `--public-url` idea. Then Relayer streams:

| Relayer RPC | Default | Opt out | What it delivers |
|-------------|---------|---------|------------------|
| `StartExpiringPacketStream` | always on | — | Leader-time / post-pack (point of no return) |
| `StartExpiringTpuPacketStream` | on | `--disable-tpu-packet-stream` | Non-leader TPU packets (**MCA** path in production) |
| `StartP2cUpdateCountStream` | on | `--disable-p2c-update-count` | Per-slot send counts on those streams |

Tell scheduler vs TPU apart by **which gRPC method** delivered the message (no `source` field). Details: [`p2c_server` README](./p2c_server/README.md).

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
| `--public-url` | Returned by `GetBlockEngineEndpoints`; must be a **public** URL |

Share that URL with Rakurai. Tip **0.001 SOL** to a [Rakurai tip account](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/tips).

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001
```

Before sharing a URL, check discovery **and** auth on each host — see [Setup guide §5.3](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide#53-how-to-check-your-server).

---

## 4. Authentication

**Purpose.** Only allowed validator identities get tokens.

**What it does.** Default allowlist: pubkey on `getLeaderSchedule` **and** advertising Rakurai `client_id` in `getClusterNodes`. Use `--allow-any-validator` for local lab only.

---

## 5. Flags

| Flag | Default | Applies to | Purpose |
|------|---------|------------|---------|
| `--bind` | `0.0.0.0:10000` (`10001` for `p2c_server`) | all | gRPC listen address |
| `--public-url` | *(set for remote validators)* | both servers | URL returned by `GetBlockEngineEndpoints` |
| `--rpc-url` (`RPC_URL`) | mainnet-beta public RPC | all | Allowlist + latest blockhash |
| `--leader-refresh-secs` | `120` | all | Allowlist refresh interval |
| `--allow-any-validator` | false | all | Skip allowlist (local / lab only) |
| `--disable-tpu-packet-stream` | false | `p2c_server` | Reject TPU Relayer RPC (`UNIMPLEMENTED`) |
| `--disable-p2c-update-count` | false | `p2c_server` | Reject per-slot count Relayer RPC (`UNIMPLEMENTED`) |

---

## 6. Layout

| Crate | Purpose |
|-------|---------|
| [`bundles_server`](./bundles_server/) | Bundles sample |
| [`p2c_server`](./p2c_server/) | P2C sample |
| [`common`](./common/) | Shared auth, allowlist, helpers |
| [`protos`](./protos/) | `.proto` + tonic codegen |
