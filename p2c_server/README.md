# `p2c_server`

**Purpose.** Sample Relayer so you can receive Rakurai post-pack (P2C) updates and decode them before building a production consumer.

**What it does.** Validators auth as `RELAYER`, open one or more packet streams on the same connection, and push transactions at the point of no return. This binary also serves `GetBlockEngineEndpoints` for region discovery. It logs each batch (validator, stream source, slot, signature, `meta.addr`) and verifies TPU `meta.addr` proofs. It does **not** send bundles — use [`bundles_server`](../bundles_server/) for that.

**Related:** [Repository overview](../README.md) · [Post-Pack Confirmations](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/readme) · [Setup guide §8](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide#8-setup-p2c-server-blockenginerelayer)

---

## 1. What it serves

| Service | Role | RPCs |
|---------|------|------|
| `auth.AuthService` | `RELAYER` | challenge / tokens |
| `block_engine.BlockEngineValidator` | discovery | `GetBlockEngineEndpoints` (`SubscribePackets` / `SubscribeBundles` → `UNIMPLEMENTED`) |
| `block_engine.BlockEngineRelayer` | `RELAYER` | leader-time, TPU, and count streams below |

### Streams (Relayer)

| RPC | Default | Enable with | Stream name | What it delivers |
|-----|---------|-------------|-------------|------------------|
| `StartExpiringPacketStream` | **always on** | — | **Resell** | Leader-time pre-conf for reselling (no boost) |
| `StartExpiringMevPacketStream` | off | `--enable-mev-packet-stream` | **Mev** | Leader-time pre-conf for backrun (boost-eligible) |
| `StartExpiringTpuPacketStream` | off | `--enable-tpu-packet-stream` | **TPU** | Non-leader transaction stream for backrun (not front-run proof) |
| `StartP2cUpdateCountStream` | off | `--enable-p2c-update-count` | Count | Per-slot counts of txs sent on those streams |

Validators multiplex whichever streams their on-chain config enables (`mev`, `resell`, `enable_tpu_p2c_update`) on **one** TLS connection per endpoint UUID. RPCs that are not enabled return `UNIMPLEMENTED`; newer validators keep the leader-time stream(s) they need.

**Production access (not enforced by this sample):**

| Path | Payment | Streams validators open |
|------|---------|-------------------------|
| Backrun / MevShare | **[MCA](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/mca)** | Mev stream + optional TPU |
| Resell pre-conf | **[PSA](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/psa)** | Resell stream only |

The default is Resell-only. For the MCA / backrun path, add `--enable-mev-packet-stream` (and optionally `--enable-tpu-packet-stream`, `--enable-p2c-update-count`).

---

## 2. What are Mev, Resell, and TPU streams?

**Purpose.** Know which product each Relayer stream is for before you wire handlers or settlement.

Post-pack (P2C) is a stream of transactions a Rakurai validator sends when a transaction reaches the **point of no return**: early enough to act on, late enough that nobody can front-run the original sender. The three packet streams below use the same wire type (`PacketBatchUpdate`); they differ by **when** the validator emits them and **what you are allowed to do** with them.

This section expands the [root README streams table](../README.md#p2c-streams-at-a-glance). Full product model: [Post-Pack Confirmations](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/readme) · [Setup guide §8.1](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide#81-differentiating-scheduler-vs-tpu).

### 2.1. Mev stream

| | |
|--|--|
| **RPC** | `StartExpiringMevPacketStream` |
| **When** | Validator is the **leader** |
| **What it is** | Standard front-run-proof pre-conf at the point of no return |
| **What it is for** | **Backrun** — consume the stream and submit reply bundles for profit |
| **Access** | Share revenue through an **[MCA](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/mca)** (Mev stream and optional TPU are enabled on this path) |
| **Virtual priority** | **Yes** — reply bundles that include the **original packet** (unchanged) and tip at the [minimum tip requirements](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/tips) get an extra **20% virtual priority**. |

Config flag on the endpoint: `mev`.

### 2.2. Resell stream

| | |
|--|--|
| **RPC** | `StartExpiringPacketStream` |
| **When** | Validator is the **leader** |
| **What it is** | Standard front-run-proof pre-conf at the point of no return (same stage as Mev) |
| **What it is for** | **Resell** — monetize orderflow by selling this pre-conf to searchers and traders |
| **Access** | Share revenue through a **[PSA](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/psa)** |
| **Virtual priority** | **No** 20% boost |
| **TPU** | Not available on the Resell path |

Config flag on the endpoint: `resell`.

### 2.3. TPU stream

| | |
|--|--|
| **RPC** | `StartExpiringTpuPacketStream` |
| **When** | Validator is **not** the leader — it forwards a packet it received on the TPU path |
| **What it is** | Non-leader transaction stream. It is **not** front-run proof and does **not** guarantee inclusion ahead of other transactions |
| **What it is for** | **Backrun** only — **do not resell** these updates to searchers as pre-conf |
| **Access** | Share revenue through an **MCA**. In production the validator also needs `enable_tpu_p2c_update` on the endpoint. Return `UNIMPLEMENTED` if you do not want this stream |

### 2.4. Side-by-side

| Stream | Leader? | Front-run proof? | Typical use | Payment | 20% virtual priority on reply? |
|--------|---------|------------------|-------------|---------|--------------------------------|
| **Mev** | Yes | Yes | Backrun | MCA | Yes, when reply includes the original packet |
| **Resell** | Yes | Yes | Resell pre-conf | PSA | No |
| **TPU** | No | **No** | Backrun only (not for resell) | MCA | No |

---

## 3. Differentiating Mev vs Resell vs TPU on the wire

**Purpose.** Know which stream delivered a batch so you apply the correct product rules (backrun vs resell vs non-leader TPU).

**What it does.** All three streams send the same `PacketBatchUpdate` type. There is no `source` field on the packet — use the gRPC method that delivered it. Official reference: [Setup guide §8.1](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide#81-differentiating-scheduler-vs-tpu) · [Using P2C](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/using_p2c).

### 3.1. Primary — which gRPC method delivered the message

| Stream | Sample log label | Meaning |
|--------|------------------|---------|
| `StartExpiringMevPacketStream` | `mev` | Mev post-pack / point of no return (boost-eligible) |
| `StartExpiringPacketStream` | `resell` | Resell post-pack / point of no return (no tip boost) |
| `StartExpiringTpuPacketStream` | `tpu` | TPU path (validator is not the leader; not front-run proof) |

The sample passes `mev`, `resell`, or `tpu` into `log_p2c_batch` from the accepting handler.

### 3.2. Secondary — `meta` on each packet

| Field | Mev / Resell | TPU |
|-------|--------------|-----|
| `meta.size` | `data.len()` | `data.len()` |
| `meta.addr` | Per-tx postpack hash string (**not** an IP) | Validator identity signature over the tx-signature string |
| `meta.port` | `0` | `0` |
| `meta.flags` | Omitted / unset | Omitted / unset |
| `meta.sender_stake` | `0` | `0` |

Treat leader-time `meta.addr` as opaque correlation data — do not parse it as a socket address. When you reply with a bundle, put the original `Packet` **unchanged** (`data` + `meta`) first. Details: [Using P2C — meta fields](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/post_pack/using_p2c#22-meta-fields).

### 3.3. Which validator sent the update

Streams are authenticated. Logs include `validator=<identity pubkey>` from the bearer token (`AuthContext.pubkey`).

### 3.4. Verifying TPU `meta.addr`

1. Decode packet → first `VersionedTransaction` signature → `txn_sig = signature.to_string()` (base58).
2. Parse `meta.addr` as a Solana `Signature` (base58).
3. `proof.verify(validator_pubkey.as_ref(), txn_sig.as_bytes())`.

Sample logs `tpu_proof=ok|bad|missing|bad_encoding`. Do **not** run this verify on Mev/Resell `meta.addr`.

```text
P2C-Update[mev]: validator=<pubkey> slot=… signature=<sig> meta.addr=<mev-hash>
P2C-Update[resell]: validator=<pubkey> slot=… signature=<sig> meta.addr=<resell-hash>
P2C-Update[tpu]: validator=<pubkey> slot=… signature=<sig> meta.addr=<proof> tpu_proof=ok
```

---

## 4. Slot on the wire (`expiry_ms`)

**Purpose.** Group updates by the leader slot that produced them.

**What it does.** Field is named `expiry_ms` for Jito wire compatibility. On P2C it holds the validator **working-bank slot** (`u32`), not a millisecond timeout. See [Setup guide §8.2](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide#82-p2c-slot-tracking-expiry_ms).

```text
expiry_ms = <slot as u32>
```

---

## 5. `P2cUpdateCount`

**Purpose.** Health / volume without counting packets yourself.

**What it does.** One message per slot (flush on disconnect) with how many txs were successfully sent on the scheduler and TPU streams. See [Setup guide §8.3](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/setup_guide#83-p2c-update-counts).

| Field | Meaning |
|-------|---------|
| `uuid` | Post-pack endpoint UUID |
| `slot` | Slot these counts belong to |
| `scheduler_count` | Txs sent on leader-time streams (Mev + Resell) that slot |
| `tpu_count` | Txs sent on the TPU stream that slot |
| `total_count` | `scheduler_count + tpu_count` |
| `p2c_tpu_enabled` | Whether TPU updates are enabled for this endpoint |

```text
P2C-UpdateCount: validator=… uuid=… slot=… scheduler_count=… tpu_count=… total_count=… p2c_tpu_enabled=…
```

---

## 6. Run

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001
```

| Flag | Default | Purpose |
|------|---------|---------|
| `--bind` | `0.0.0.0:10001` | gRPC listen address |
| `--public-url` | `http://127.0.0.1:10001` | Returned by discovery (use a **public** URL for real validators) |
| `--enable-mev-packet-stream` | off | Accept Mev stream |
| `--enable-tpu-packet-stream` | off | Accept TPU stream |
| `--enable-p2c-update-count` | off | Accept count stream |
| `--tls-cert` / `--tls-key` (`TLS_CERT` / `TLS_KEY`) | unset (no TLS) | PEM cert + key; set both to serve TLS (use an `https://` public URL) |
| `--rpc-url` / `RPC_URL` | mainnet-beta | Leader allowlist |
| `--allow-any-validator` | false | Skip allowlist (local only) |

Register the discovery URL with Rakurai. Replace logging with your backrun / reply-bundle logic. Reply bundles that include the original Mev packet get an additional **20% virtual priority** on Rakurai — see [§2.1](#21-mev-stream) and [Tips](https://docs.rakurai.io/docs/services/rakurai_jito_private/rakurai_docs/transaction_inclusion/tips).

### Example: Mev + TPU + counts (MCA / backrun)

The default command above is Resell-only (PSA). To accept every stream:

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url http://<HOST_IP>:10001 \
  --enable-mev-packet-stream \
  --enable-tpu-packet-stream \
  --enable-p2c-update-count
```

### Example: TLS

```bash
RUST_LOG=info cargo run --release -p p2c_server -- \
  --bind 0.0.0.0:10001 \
  --public-url https://<HOST>:10001 \
  --tls-cert /path/fullchain.pem \
  --tls-key /path/privkey.pem
```
