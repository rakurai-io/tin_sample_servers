//! Sample `p2c_server` — post-pack confirmation (P2C) updates receiver.
//!
//! Exposes:
//! - `auth.AuthService` with role `RELAYER`
//! - `block_engine.BlockEngineValidator` (`GetBlockEngineEndpoints` for P2C autoconfig)
//! - `block_engine.BlockEngineRelayer` (`StartExpiringPacketStream`, optional TPU + count streams)

use {
    anyhow::Context,
    clap::Parser,
    common::{
        AuthServiceImpl, LeaderScheduleCache, TokenStore, auth::require_bearer, log_p2c_batch,
        tokens::default_ttls, warn_if_bad_public_url,
    },
    log::{info, warn},
    protos::{
        auth::{Role, auth_service_server::AuthServiceServer},
        block_engine::{
            AccountsOfInterestRequest, AccountsOfInterestUpdate, BlockBuilderFeeInfoRequest,
            BlockBuilderFeeInfoResponse, BlockEngineEndpoint, GetBlockEngineEndpointRequest,
            GetBlockEngineEndpointResponse, P2cUpdateCount, PacketBatchUpdate,
            ProgramsOfInterestRequest, ProgramsOfInterestUpdate, StartExpiringPacketStreamResponse,
            SubscribeBundlesRequest, SubscribeBundlesResponse, SubscribePacketsRequest,
            SubscribePacketsResponse,
            block_engine_relayer_server::{BlockEngineRelayer, BlockEngineRelayerServer},
            block_engine_validator_server::{BlockEngineValidator, BlockEngineValidatorServer},
        },
        shared::Heartbeat,
    },
    std::{net::SocketAddr, sync::Arc, time::Duration},
    tokio::sync::mpsc,
    tokio_stream::{StreamExt, wrappers::ReceiverStream},
    tonic::{Request, Response, Status, Streaming, transport::Server},
};

#[derive(Debug, Parser)]
#[command(
    name = "p2c_server",
    about = "TIN sample — receive post-pack confirmations"
)]
struct Args {
    /// Listen address, e.g. 0.0.0.0:10001
    #[arg(long, default_value = "0.0.0.0:10001")]
    bind: SocketAddr,

    /// Public URL returned by GetBlockEngineEndpoints (must be reachable from validators).
    #[arg(long, default_value = "http://127.0.0.1:10001")]
    public_url: String,

    /// Solana JSON-RPC URL used to refresh the leader schedule.
    #[arg(
        long,
        env = "RPC_URL",
        default_value = "https://api.mainnet-beta.solana.com"
    )]
    rpc_url: String,

    /// Skip Rakurai leader allowlist (local testing only).
    #[arg(long, default_value_t = false)]
    allow_any_validator: bool,

    /// How often to refresh leader schedule + getClusterNodes.
    #[arg(long, default_value_t = 120)]
    leader_refresh_secs: u64,

    /// Disable StartExpiringTpuPacketStream (enabled by default).
    #[arg(long, default_value_t = false)]
    disable_tpu_packet_stream: bool,

    /// Disable StartP2cUpdateCountStream (enabled by default).
    #[arg(long, default_value_t = false)]
    disable_p2c_update_count: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let (challenge_ttl, access_ttl, refresh_ttl) = default_ttls();
    let tokens = Arc::new(TokenStore::new(challenge_ttl, access_ttl, refresh_ttl));
    let leaders = LeaderScheduleCache::new(
        args.rpc_url.clone(),
        args.allow_any_validator,
        Duration::from_secs(args.leader_refresh_secs),
    );

    let auth = AuthServiceImpl::new(tokens.clone(), leaders, vec![Role::Relayer]);
    let validator = BlockEngineValidatorImpl {
        public_url: args.public_url.clone(),
    };
    let relayer = BlockEngineRelayerImpl {
        tokens,
        enable_tpu_packet_stream: !args.disable_tpu_packet_stream,
        enable_p2c_update_count: !args.disable_p2c_update_count,
    };

    warn_if_bad_public_url(&args.public_url);
    info!(
        "P2C Server: listening on {} public_url={} tpu_stream={} update_count={}",
        args.bind,
        args.public_url,
        !args.disable_tpu_packet_stream,
        !args.disable_p2c_update_count
    );
    Server::builder()
        .add_service(AuthServiceServer::new(auth))
        .add_service(BlockEngineValidatorServer::new(validator))
        .add_service(BlockEngineRelayerServer::new(relayer))
        .serve(args.bind)
        .await
        .context("gRPC serve")?;
    Ok(())
}

/// Discovery-only Validator surface for P2C autoconfig (`GetBlockEngineEndpoints`).
struct BlockEngineValidatorImpl {
    public_url: String,
}

#[tonic::async_trait]
impl BlockEngineValidator for BlockEngineValidatorImpl {
    type SubscribePacketsStream = ReceiverStream<Result<SubscribePacketsResponse, Status>>;
    type SubscribeBundlesStream = ReceiverStream<Result<SubscribeBundlesResponse, Status>>;

    async fn subscribe_packets(
        &self,
        _request: Request<SubscribePacketsRequest>,
    ) -> Result<Response<Self::SubscribePacketsStream>, Status> {
        Err(Status::unimplemented(
            "p2c_server does not push packets; use bundles_server",
        ))
    }

    async fn subscribe_bundles(
        &self,
        _request: Request<SubscribeBundlesRequest>,
    ) -> Result<Response<Self::SubscribeBundlesStream>, Status> {
        Err(Status::unimplemented(
            "p2c_server does not push bundles; use bundles_server",
        ))
    }

    async fn get_block_builder_fee_info(
        &self,
        _request: Request<BlockBuilderFeeInfoRequest>,
    ) -> Result<Response<BlockBuilderFeeInfoResponse>, Status> {
        Err(Status::unimplemented(
            "p2c_server does not serve block builder fees",
        ))
    }

    async fn get_block_engine_endpoints(
        &self,
        request: Request<GetBlockEngineEndpointRequest>,
    ) -> Result<Response<GetBlockEngineEndpointResponse>, Status> {
        let _ = request;
        Ok(Response::new(GetBlockEngineEndpointResponse {
            global_endpoint: Some(BlockEngineEndpoint {
                block_engine_url: self.public_url.clone(),
                shredstream_receiver_address: String::new(),
            }),
            regioned_endpoints: vec![BlockEngineEndpoint {
                block_engine_url: self.public_url.clone(),
                shredstream_receiver_address: String::new(),
            }],
        }))
    }
}

struct BlockEngineRelayerImpl {
    tokens: Arc<TokenStore>,
    enable_tpu_packet_stream: bool,
    enable_p2c_update_count: bool,
}

#[tonic::async_trait]
impl BlockEngineRelayer for BlockEngineRelayerImpl {
    type SubscribeAccountsOfInterestStream =
        ReceiverStream<Result<AccountsOfInterestUpdate, Status>>;
    type SubscribeProgramsOfInterestStream =
        ReceiverStream<Result<ProgramsOfInterestUpdate, Status>>;
    type StartExpiringPacketStreamStream =
        ReceiverStream<Result<StartExpiringPacketStreamResponse, Status>>;
    type StartExpiringTpuPacketStreamStream =
        ReceiverStream<Result<StartExpiringPacketStreamResponse, Status>>;
    type StartP2cUpdateCountStreamStream =
        ReceiverStream<Result<StartExpiringPacketStreamResponse, Status>>;

    async fn subscribe_accounts_of_interest(
        &self,
        request: Request<AccountsOfInterestRequest>,
    ) -> Result<Response<Self::SubscribeAccountsOfInterestStream>, Status> {
        let _ctx = require_bearer(&request, &self.tokens, Role::Relayer)?;
        let (tx, rx) = mpsc::channel(4);
        let _ = tx
            .send(Ok(AccountsOfInterestUpdate {
                accounts: vec!["*".to_string()],
            }))
            .await;
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn subscribe_programs_of_interest(
        &self,
        request: Request<ProgramsOfInterestRequest>,
    ) -> Result<Response<Self::SubscribeProgramsOfInterestStream>, Status> {
        let _ctx = require_bearer(&request, &self.tokens, Role::Relayer)?;
        let (tx, rx) = mpsc::channel(4);
        let _ = tx
            .send(Ok(ProgramsOfInterestUpdate {
                programs: vec!["*".to_string()],
            }))
            .await;
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn start_expiring_packet_stream(
        &self,
        request: Request<Streaming<PacketBatchUpdate>>,
    ) -> Result<Response<Self::StartExpiringPacketStreamStream>, Status> {
        serve_packet_stream(request, &self.tokens, "scheduler").await
    }

    async fn start_expiring_tpu_packet_stream(
        &self,
        request: Request<Streaming<PacketBatchUpdate>>,
    ) -> Result<Response<Self::StartExpiringTpuPacketStreamStream>, Status> {
        if !self.enable_tpu_packet_stream {
            return Err(Status::unimplemented(
                "StartExpiringTpuPacketStream disabled (--disable-tpu-packet-stream)",
            ));
        }
        serve_packet_stream(request, &self.tokens, "tpu").await
    }

    async fn start_p2c_update_count_stream(
        &self,
        request: Request<Streaming<P2cUpdateCount>>,
    ) -> Result<Response<Self::StartP2cUpdateCountStreamStream>, Status> {
        if !self.enable_p2c_update_count {
            return Err(Status::unimplemented(
                "StartP2cUpdateCountStream disabled (--disable-p2c-update-count)",
            ));
        }

        let ctx = require_bearer(&request, &self.tokens, Role::Relayer)?;
        info!(
            "P2C Server: StartP2cUpdateCountStream from {}",
            ctx.pubkey
        );

        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel(32);
        spawn_server_heartbeats(tx.clone());

        tokio::spawn(async move {
            while let Some(msg) = inbound.next().await {
                match msg {
                    Ok(count) => {
                        info!(
                            "P2C-UpdateCount: validator={} uuid={} slot={} scheduler_count={} \
                             tpu_count={} total_count={} p2c_tpu_enabled={}",
                            ctx.pubkey,
                            count.uuid,
                            count.slot,
                            count.scheduler_count,
                            count.tpu_count,
                            count.total_count,
                            count.p2c_tpu_enabled
                        );
                    }
                    Err(err) => {
                        warn!("P2C Server: update-count stream error: {err}");
                        break;
                    }
                }
            }
            info!(
                "P2C Server: StartP2cUpdateCountStream closed for {}",
                ctx.pubkey
            );
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

async fn serve_packet_stream(
    request: Request<Streaming<PacketBatchUpdate>>,
    tokens: &Arc<TokenStore>,
    source: &'static str,
) -> Result<Response<ReceiverStream<Result<StartExpiringPacketStreamResponse, Status>>>, Status> {
    let ctx = require_bearer(&request, tokens, Role::Relayer)?;
    info!(
        "P2C Server: StartExpiringPacketStream ({source}) from {}",
        ctx.pubkey
    );

    let mut inbound = request.into_inner();
    let (tx, rx) = mpsc::channel(32);
    spawn_server_heartbeats(tx.clone());

    tokio::spawn(async move {
        while let Some(msg) = inbound.next().await {
            match msg {
                Ok(PacketBatchUpdate { msg: Some(m) }) => match m {
                    protos::block_engine::packet_batch_update::Msg::Batches(batch) => {
                        let _packets = log_p2c_batch(&batch, source, &ctx.pubkey);
                        // Replace with your backrun / reply-bundle logic.
                    }
                    protos::block_engine::packet_batch_update::Msg::Heartbeat(hb) => {
                        info!("P2C Server: client heartbeat ({source}) count={}", hb.count);
                    }
                },
                Ok(_) => {}
                Err(err) => {
                    warn!("P2C Server: {source} packet stream error: {err}");
                    break;
                }
            }
        }
        info!(
            "P2C Server: packet stream ({source}) closed for {}",
            ctx.pubkey
        );
    });

    Ok(Response::new(ReceiverStream::new(rx)))
}

fn spawn_server_heartbeats(tx: mpsc::Sender<Result<StartExpiringPacketStreamResponse, Status>>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        let mut count = 0u64;
        loop {
            tick.tick().await;
            count = count.wrapping_add(1);
            let msg = StartExpiringPacketStreamResponse {
                heartbeat: Some(Heartbeat { count }),
            };
            if tx.send(Ok(msg)).await.is_err() {
                break;
            }
        }
    });
}
