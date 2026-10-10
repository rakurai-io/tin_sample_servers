# `p2c_server`

**Purpose.** Sample Relayer so you can receive Rakurai post-pack (P2C) updates and decoding them before building a production consumer.

**What it does.** Validators auth as `RELAYER`, open one or more packet streams on the same connection, and push transactions at the point of no return. This binary also serves `GetBlockEngineEndpoints` for region discovery. It logs each batch (validator, stream source, slot, signature, `meta.addr`) and verifies TPU `meta.addr` proofs. It does **not** send bundles — use [`bundles_server`](../bundles_server/) for that.

---

## 1. What it serves

| Service | Role | RPCs |
|---------|------|------|
| `auth.AuthService` | `RELAYER` | challenge / tokens |
| `block_engine.BlockEngineValidator` | discovery | `GetBlockEngineEndpoints` (`SubscribePackets` / `SubscribeBundles` → `UNIMPLEMENTED`) |
| `block_engine.BlockEngineRelayer` | `RELAYER` | leader-time, TPU, and count streams below |

### Streams (Relayer)

| RPC | Default | Disable with | What it delivers |
|-----|---------|--------------|------------------|
| `StartExpiringPacketStream` | always on | — | ReSell leader-time / post-pack (no-boost hash) |
| `StartExpiringMevPacketStream` | always on | — | Mev leader-time / post-pack (boost hash) |
| `StartExpiringTpuPacketStream` | **on** | `--disable-tpu-packet-stream` | Non-leader TPU packets |
| `StartP2cUpdateCountStream` | **on** | `--disable-p2c-update-count` | Per-slot counts of txs sent on those streams |

Validators multiplex whichever streams their on-chain config enables (`resell`, `mev`, `enable_tpu_p2c_update`) on **one** TLS connection per endpoint UUID. Disabled optional RPCs return `UNIMPLEMENTED`; newer validators keep the leader-time stream(s) they need.

**Production access (not enforced by this sample):**

| Path | Payment | Streams validators open |
|------|---------|-------------------------|
| Pre-conf / reselling | **PSA** | `StartExpiringPacketStream` only |
| Backrun / MevShare | **MCA** (**PSA included**) | `StartExpiringMevPacketStream` + optional TPU |
| Both flags on one URL | PSA + MCA | Packet + Mev streams (distinct hashes); TPU only with Mev flag |

For local PSA-style testing, run with `--disable-tpu-packet-stream` (and optionally `--disable-p2c-update-count`).

---

## 2. Differentiating resell vs mev vs TPU

**Purpose.** Know whether an update is ReSell post-pack, Mev post-pack, or earlier TPU traffic.

**What it does.** All streams send the same `PacketBatchUpdate` type. There is no `source` field on the packet — use the gRPC method that delivered it.

### 2.1. Primary — which gRPC method delivered the message

| Stream | Sample log label | Meaning |
|--------|------------------|---------|
| `StartExpiringPacketStream` | `resell` | ReSell post-pack / point of no return (no tip boost) |
| `StartExpiringMevPacketStream` | `mev` | Mev post-pack / boost-eligible hash |
| `StartExpiringTpuPacketStream` | `tpu` | TPU path (validator is not the leader) |

The sample passes `resell`, `mev`, or `tpu` into `log_p2c_batch` from the accepting handler.

### 2.2. Secondary — `meta` on each packet

| Field | ReSell / Mev | TPU |
|-------|--------------|-----|
| `meta.size` | `data.len()` | `data.len()` |
| `meta.addr` | Per-tx postpack hash string (**not** an IP) | Validator identity signature over the tx-signature string |
| `meta.port` | `0` | `0` |
| `meta.flags` | Omitted / unset | Omitted / unset |
| `meta.sender_stake` | `0` | `0` |

Treat leader-time `meta.addr` as opaque correlation data — do not parse it as a socket address. When you reply with a bundle, put the original `Packet` **unchanged** (`data` + `meta`) first.

### 2.3. Which validator sent the update

Streams are authenticated. Logs include `validator=<identity pubkey>` from the bearer token (`AuthContext.pubkey`).

### 2.4. Verifying TPU `meta.addr`

1. Decode packet → first `VersionedTransaction` signature → `txn_sig = signature.to_string()` (base58).
2. Parse `meta.addr` as a Solana `Signature` (base58).
3. `proof.verify(validator_pubkey.as_ref(), txn_sig.as_bytes())`.

Sample logs `tpu_proof=ok|bad|missing|bad_encoding`. Do **not** run this verify on ReSell/Mev `meta.addr`.

```text
P2C-Update[resell]: validator=<pubkey> slot=… signature=<sig> meta.addr=<resell-hash>
P2C-Update[mev]: validator=<pubkey> slot=… signature=<sig> meta.addr=<mev-hash>
P2C-Update[tpu]: validator=<pubkey> slot=… signature=<sig> meta.addr=<proof> tpu_proof=ok
```

---

## 3. Slot on the wire (`expiry_ms`)

**Purpose.** Group updates by the leader slot that produced them.

**What it does.** Field is named `expiry_ms` for Jito wire compatibility. On P2C it holds the validator **working-bank slot** (`u32`), not a millisecond timeout.

```text
expiry_ms = <slot as u32>
```

---

## 4. `P2cUpdateCount`

**Purpose.** Health / volume without counting packets yourself.

**What it does.** One message per slot (flush on disconnect) with how many txs were successfully sent on the scheduler and TPU streams.

| Field | Meaning |
|-------|---------|
| `uuid` | Post-pack endpoint UUID |
| `slot` | Slot these counts belong to |
| `scheduler_count` | Txs sent on leader-time streams (Packet + Mev) that slot |
| `tpu_count` | Txs sent on the TPU stream that slot |
| `total_count` | `scheduler_count + tpu_count` |
| `p2c_tpu_enabled` | Whether TPU updates are enabled for this endpoint |

```text
P2C-UpdateCount: validator=… uuid=… slot=… scheduler_count=… tpu_count=… total_count=… p2c_tpu_enabled=…
```

---

## 5. Run

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001
```

| Flag | Default | Purpose |
|------|---------|---------|
| `--bind` | `0.0.0.0:10001` | gRPC listen address |
| `--public-url` | `http://127.0.0.1:10001` | Returned by discovery (use a **public** URL for real validators) |
| `--disable-tpu-packet-stream` | off | Reject TPU stream |
| `--disable-p2c-update-count` | off | Reject count stream |
| `--rpc-url` / `RPC_URL` | mainnet-beta | Leader allowlist |
| `--allow-any-validator` | false | Skip allowlist (local only) |

Register the discovery URL with Rakurai. Replace logging with your backrun / reply-bundle logic. Backrun bundles that use **leader-time Mev source txs** get an additional **20% virtual priority** on Rakurai.

### Example: ReSell leader-time only (PSA-style / no TPU)

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001 \
  --disable-tpu-packet-stream \
  --disable-p2c-update-count
```

(Mev validators still need `StartExpiringMevPacketStream`; this sample keeps that RPC enabled unless you fork the binary.)
