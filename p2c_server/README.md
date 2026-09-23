# `p2c_server`

**Post-pack confirmation (P2C) sample.** Implements the TIN Relayer path: validators authenticate with role `RELAYER` and open packet streams from the point of no return. Also serves `GetBlockEngineEndpoints` on `BlockEngineValidator` so validators can autoconfig / region-rank your P2C URL (same discovery RPC as bundles).

The sample parses each batch and logs **validator** (from Relayer auth), **source** (`scheduler` / `tpu` — set by which gRPC method accepted the stream), **slot**, transaction signature, and `meta.addr`. On the TPU stream it also **verifies** `meta.addr`. It does **not** push bundles to validators — use [`bundles_server`](../bundles_server/) for the Validator / block-engine path.

**Related:** [Repository overview](../README.md) · [Setup guide](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide) · [Using P2C](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/using_p2c)

---

## 1. What it serves

| Service | Role | RPCs |
|---------|------|------|
| `auth.AuthService` | `RELAYER` | challenge / tokens |
| `block_engine.BlockEngineValidator` | *(unauthenticated discovery)* | `GetBlockEngineEndpoints` (`SubscribePackets` / `SubscribeBundles` return `UNIMPLEMENTED`) |
| `block_engine.BlockEngineRelayer` | `RELAYER` | `StartExpiringPacketStream` (scheduler), `StartExpiringTpuPacketStream` (TPU), `StartP2cUpdateCountStream` (per-slot counts) |

### Streams (Relayer)

| RPC | Default | Disable with | What it delivers |
|-----|---------|--------------|------------------|
| `StartExpiringPacketStream` | always on | — | Point-of-no-return transactions while this validator is the leader |
| `StartExpiringTpuPacketStream` | **on** | `--disable-tpu-packet-stream` | Transactions seen on TPU while this validator is **not** the leader (same `PacketBatchUpdate` wire type) |
| `StartP2cUpdateCountStream` | **on** | `--disable-p2c-update-count` | Once per slot: how many transactions were successfully sent on the scheduler and TPU streams |

When disabled, the RPC returns `UNIMPLEMENTED` so newer validators keep the scheduler stream and skip the optional ones.

---

## 2. Differentiating scheduler vs TPU

Both Relayer streams send the same `PacketBatchUpdate` message type. There is no `source` field on the packet. Tell the paths apart as follows.

### 2.1. Primary — which gRPC method delivered the message

| Stream | Sample log label | Meaning |
|--------|------------------|---------|
| `StartExpiringPacketStream` | `scheduler` | Post-pack / point of no return for this leader slot |
| `StartExpiringTpuPacketStream` | `tpu` | Earlier TPU path (validator is not the leader) |

This sample passes `"scheduler"` or `"tpu"` into `log_p2c_batch` from the handler that accepted the stream. Do **not** look for a packet field named `source`.

### 2.2. Secondary — `meta` on each packet

| Field | Scheduler | TPU |
|-------|-----------|-----|
| `meta.size` | `data.len()` | `data.len()` |
| `meta.addr` | Per-tx string from the scheduler (**not** an IP) | **Validator identity signature** over the transaction-signature string (proof this leader emitted the update) |
| `meta.port` | `0` | `0` |
| `meta.flags` | Omitted / unset | Omitted / unset |
| `meta.sender_stake` | `0` | `0` |

Treat `meta.addr` as opaque data for proof / correlation — never parse it as a socket address (despite the field name). When you reply with a bundle, put the original `Packet` **unchanged** (`data` + `meta`) first.

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

## 3. Slot on the wire (`expiry_ms`)

Each `ExpiringPacketBatch` carries a field named `expiry_ms` (kept for Jito wire compatibility):

```text
expiry_ms = <slot as u32>
```

| What the value is | What it is **not** |
|-------------------|--------------------|
| The validator’s **working-bank slot** when the update was sent | A millisecond expiry / TTL |

Use it to group, debounce, and time reply bundles against the leader slot that produced the update. When the slot rolls, treat it as a new window (same idea as the count stream).

---

## 4. `P2cUpdateCount`

When the count stream is enabled, the validator pushes one message **per slot** (and flushes on disconnect). Each message reports how many transactions were successfully sent on the P2C update streams for that slot:

| Field | Meaning |
|-------|---------|
| `uuid` | Post-pack endpoint UUID |
| `slot` | Slot these counts belong to |
| `scheduler_count` | Transactions successfully sent on the scheduler stream that slot |
| `tpu_count` | Transactions successfully sent on the TPU stream that slot |
| `total_count` | `scheduler_count + tpu_count` |
| `p2c_tpu_enabled` | Whether TPU updates are enabled for this endpoint |

Use this as a health / volume signal without counting packets yourself — confirm the leader is connected and how much scheduler vs TPU traffic you received that slot.

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
