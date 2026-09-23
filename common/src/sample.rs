//! Dummy packet / bundle helpers shared by the sample binaries.

use {
    anyhow::Context,
    log::{info, warn},
    protos::{
        block_engine::ExpiringPacketBatch,
        bundle::{Bundle, BundleUuid},
        packet::{Meta, Packet},
    },
    solana_keypair::Keypair,
    solana_message::Message,
    solana_pubkey::Pubkey,
    solana_rpc_client::rpc_client::RpcClient,
    solana_signature::Signature,
    solana_signer::Signer,
    solana_system_interface::instruction::transfer,
    solana_transaction::{Transaction, versioned::VersionedTransaction},
    std::{str::FromStr, time::Duration},
    uuid::Uuid,
};

/// One of the eight mainnet Rakurai tip accounts (see tips docs).
pub const RAKURAI_TIP_ACCOUNT: &str = "BjqjPHFmwr19YFmkH8CMNJFbj1wzX9k9ngr4am2nQEdq";

/// Warn when `--public-url` is not reachable from remote validators.
///
/// `GetBlockEngineEndpoints` returns this URL; the validator reconnects to it after discovery.
/// `0.0.0.0` / `127.0.0.1` / `localhost` make autoconfig fail with
/// "all N candidate endpoints failed to connect".
pub fn warn_if_bad_public_url(public_url: &str) {
    let lower = public_url.to_ascii_lowercase();
    if lower.contains("0.0.0.0")
        || lower.contains("127.0.0.1")
        || lower.contains("localhost")
        || lower.contains("[::]")
        || lower.contains("[::1]")
    {
        warn!(
            "--public-url={public_url} is not reachable from remote validators; \
             use your public host (e.g. http://<public-ip>:10001). \
             Bind may stay 0.0.0.0; public-url must be the address validators connect."
        );
    }
}

pub fn packet_bytes(packet: &Packet) -> &[u8] {
    match packet.meta.as_ref() {
        Some(meta) if meta.size > 0 && (meta.size as usize) <= packet.data.len() => {
            &packet.data[..meta.size as usize]
        }
        _ => &packet.data,
    }
}

/// Decode + log each P2C packet; returns the original packets (unchanged).
///
/// `source` is `scheduler` or `tpu`. `validator` is the identity from Relayer auth
/// (bearer token). `expiry_ms` is the working-bank slot on the P2C path.
///
/// On the **TPU** stream, `meta.addr` is the validator identity signature over the
/// transaction-signature string — verified here and logged as `tpu_proof=ok|bad|…`.
pub fn log_p2c_batch(
    batch: &ExpiringPacketBatch,
    source: &str,
    validator: &Pubkey,
) -> Vec<Packet> {
    let Some(packet_batch) = batch.batch.as_ref() else {
        warn!(
            "P2C-Update[{source}]: validator={validator} PacketBatchUpdate with no packets \
             slot/expiry_ms={}",
            batch.expiry_ms
        );
        return Vec::new();
    };

    let mut out = Vec::with_capacity(packet_batch.packets.len());
    for packet in &packet_batch.packets {
        let meta_addr = packet.meta.as_ref().map(|m| m.addr.as_str()).unwrap_or("");
        match bincode::deserialize::<VersionedTransaction>(packet_bytes(packet)) {
            Ok(txn) => {
                let sig = txn
                    .signatures
                    .first()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "<no-signature>".to_string());
                if source == "tpu" {
                    let proof = verify_tpu_meta_addr(validator, &sig, meta_addr);
                    info!(
                        "P2C-Update[{source}]: validator={validator} slot={} signature={sig} \
                         meta.addr={meta_addr} tpu_proof={proof}",
                        batch.expiry_ms
                    );
                } else {
                    info!(
                        "P2C-Update[{source}]: validator={validator} slot={} signature={sig} \
                         meta.addr={meta_addr}",
                        batch.expiry_ms
                    );
                }
                out.push(packet.clone());
            }
            Err(err) => warn!(
                "P2C-Update[{source}]: validator={validator} slot={} meta.addr={meta_addr} \
                 failed to deserialize packet ({} bytes): {err}",
                batch.expiry_ms,
                packet_bytes(packet).len()
            ),
        }
    }
    out
}

/// TPU `meta.addr` = identity `Signature` over the txn-signature **string** bytes.
/// Message is `signature.to_string().as_bytes()` (same as the validator signs).
fn verify_tpu_meta_addr(validator: &Pubkey, txn_sig: &str, meta_addr: &str) -> &'static str {
    if meta_addr.is_empty() {
        return "missing";
    }
    let Ok(proof) = Signature::from_str(meta_addr) else {
        return "bad_encoding";
    };
    if proof.verify(validator.as_ref(), txn_sig.as_bytes()) {
        "ok"
    } else {
        "bad"
    }
}

/// Dummy tip-only bundle: throwaway `Keypair::new()` tips 0.001 SOL using a live blockhash.
pub fn make_dummy_bundle(rpc_url: &str) -> anyhow::Result<(BundleUuid, String)> {
    let (tip_packet, tip_sig) = make_dummy_tip_packet(rpc_url)?;
    Ok((
        BundleUuid {
            uuid: Uuid::new_v4().to_string(),
            bundle: Some(Bundle {
                header: None,
                packets: vec![tip_packet],
            }),
        },
        tip_sig,
    ))
}

fn make_dummy_tip_packet(rpc_url: &str) -> anyhow::Result<(Packet, String)> {
    let payer = Keypair::new();
    let tip = Pubkey::from_str(RAKURAI_TIP_ACCOUNT).context("tip account")?;
    let client = RpcClient::new_with_timeout(rpc_url.to_string(), Duration::from_secs(30));
    let blockhash = client
        .get_latest_blockhash()
        .context("get_latest_blockhash")?;
    let ix = transfer(&payer.pubkey(), &tip, 1_000_000);
    let message = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[&payer], message, blockhash);
    let vtx = VersionedTransaction::from(tx);
    let sig = vtx
        .signatures
        .first()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "<no-signature>".to_string());
    let data = bincode::serialize(&vtx).context("serialize VersionedTransaction")?;
    let size = data.len() as u64;
    Ok((
        Packet {
            data: data.into(),
            meta: Some(Meta {
                size,
                addr: String::new(),
                port: 0,
                flags: None,
                sender_stake: 0,
            }),
        },
        sig,
    ))
}
