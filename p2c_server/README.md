# `p2c_server`

**Post-pack confirmation (P2C) sample.** Implements the TIN Relayer path: validators authenticate with role `RELAYER` and open packet streams from the point of no return. Also serves `GetBlockEngineEndpoints` on `BlockEngineValidator` so validators can autoconfig / region-rank your P2C URL (same discovery RPC as bundles).

The sample parses each batch, logs **validator** (from Relayer auth), **source** (`scheduler` / `tpu`), **slot**, transaction signature, and `meta.addr`. On the TPU stream it also **verifies** `meta.addr`. It does **not** push bundles to validators — use [`bundles_server`](../bundles_server/) for the Validator/block-engine path.

**Related:** [Repository overview](../README.md) · [Setup guide](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide) · [Using P2C](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/using_p2c)

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

---

## 2. Differentiating scheduler vs TPU

Wire `PacketBatchUpdate` is the **same** on both streams. Differentiate like this:

### 2.1. Primary — which RPC you accepted

| Stream | Sample log label | Meaning |
|--------|------------------|---------|
| `StartExpiringPacketStream` | `scheduler` | Post-pack / point of no return for this leader slot |
| `StartExpiringTpuPacketStream` | `tpu` | Earlier TPU / sigverify path |

This sample passes `"scheduler"` or `"tpu"` into `log_p2c_batch` from the handler that accepted the stream. Do **not** look for a packet field named “source”.

### 2.2. Secondary — `meta` on each packet

| Field | Scheduler | TPU |
|-------|-----------|-----|
| `meta.size` | `data.len()` | `data.len()` |
| `meta.addr` | Per-tx string from the scheduler (**not** an IP) | **Validator identity signature** over the transaction-signature string (proof this leader emitted the update) |
| `meta.port` | `0` | `0` |
| `meta.flags` | Omitted / unset | Omitted / unset |
| `meta.sender_stake` | `0` | `0` |

Treat `meta.addr` as opaque proof / correlation data — never parse it as a socket address. When you reply with a bundle, put the original `Packet` **unchanged** (`data` + `meta`) first.

### 2.3. Which validator sent the update

Every Relayer stream is authenticated. The sample logs `validator=<identity pubkey>` from the bearer token (`AuthContext.pubkey`) on each batch and count message — that is the validator that opened the stream.

### 2.4. Verifying TPU `meta.addr`

On the TPU path the validator signs the **transaction signature string** with its identity keypair and puts the proof in `meta.addr`:

1. Decode the packet → first `VersionedTransaction` signature → `txn_sig = signature.to_string()` (base58).
2. Parse `meta.addr` as a Solana `Signature` (base58).
3. `proof.verify(validator_pubkey.as_ref(), txn_sig.as_bytes())` — `validator_pubkey` is the same identity from auth.

The sample does this in `log_p2c_batch` and logs `tpu_proof=ok|bad|missing|bad_encoding`.

```text
P2C-Update[scheduler]: validator=<pubkey> slot=… signature=<sig> meta.addr=<scheduler-string>
P2C-Update[tpu]: validator=<pubkey> slot=… signature=<sig> meta.addr=<proof> tpu_proof=ok
```

Scheduler `meta.addr` is **not** an identity proof — do not run the TPU verify path on it.

---

## 3. Slot on the wire (`expiry_ms`) — what and why

Each `ExpiringPacketBatch` carries:

```text
expiry_ms = <working-bank slot as u32>
```

| What it is | What it is **not** |
|------------|--------------------|
| The validator **working-bank slot** when the update was sent | A millisecond expiry / TTL |

**Why:** group, debounce, and time replies against the leader slot that produced the update — without a separate slot field on every packet. When the slot rolls, treat it as a new window (same idea as the count stream).

---

## 4. `P2cUpdateCount` — what and why

When the count stream is enabled, the validator pushes one message **per slot** (and flushes on disconnect):

| Field | Meaning |
|-------|---------|
| `uuid` | Post-pack endpoint UUID |
| `slot` | Slot these counts belong to |
| `scheduler_count` | Successful sends on the scheduler stream that slot |
| `tpu_count` | Successful sends on the TPU stream that slot |
| `total_count` | `scheduler_count + tpu_count` |
| `p2c_tpu_enabled` | Whether TPU updates are enabled for this endpoint |

**Why:** health / volume signal without counting packets yourself — confirm the leader is connected and how much scheduler vs TPU traffic you saw that slot.

Sample log:

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
| `--public-url` | `http://127.0.0.1:10001` | Returned by `GetBlockEngineEndpoints` (must be reachable from validators) |
| `--disable-tpu-packet-stream` | off | Reject TPU packet stream |
| `--disable-p2c-update-count` | off | Reject per-slot count stream |
| `--rpc-url` / `RPC_URL` | mainnet-beta | Leader allowlist |
| `--allow-any-validator` | false | Skip allowlist (local only) |

Register the **discovery** URL with Rakurai for P2C (same idea as bundles: one URL; you advertise `global_endpoint` / `regioned_endpoints` from this process). Replace signature logging with your backrun / reply-bundle logic.

### Example: scheduler-only (no TPU, no counts)

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001 \
  --disable-tpu-packet-stream \
  --disable-p2c-update-count
```
