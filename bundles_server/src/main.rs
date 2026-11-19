//! Sample `bundles_server` — bundles path only.
//!
//! Exposes:
//! - `auth.AuthService` with role `VALIDATOR`
//! - `block_engine.BlockEngineValidator` (`SubscribePackets` / `SubscribeBundles` /
//!   `GetBlockEngineEndpoints`)

use {
    anyhow::Context,
    clap::Parser,
    common::{
        AuthServiceImpl, LeaderScheduleCache, TokenStore, auth::require_bearer, make_dummy_bundle,
        TlsArgs, server_builder,
        tokens::{PURGE_INTERVAL, default_ttls},
        warn_if_bad_public_url,
    },
    log::{info, warn},
    protos::{
        auth::{Role, auth_service_server::AuthServiceServer},
        block_engine::{
            BlockBuilderFeeInfoRequest, BlockBuilderFeeInfoResponse, BlockEngineEndpoint,
            GetBlockEngineEndpointRequest, GetBlockEngineEndpointResponse, SubscribeBundlesRequest,
            SubscribeBundlesResponse, SubscribePacketsRequest, SubscribePacketsResponse,
            block_engine_validator_server::{BlockEngineValidator, BlockEngineValidatorServer},
        },
    },
    std::{net::SocketAddr, sync::Arc, time::Duration},
    tokio::sync::mpsc,
    tokio_stream::wrappers::ReceiverStream,
    tonic::{Request, Response, Status},
};

#[derive(Debug, Parser)]
#[command(
    name = "bundles_server",
    about = "TIN sample — push packets/bundles to validators"
)]
struct Args {
    /// Listen address, e.g. 0.0.0.0:10000
    #[arg(long, default_value = "0.0.0.0:10000")]
    bind: SocketAddr,

    /// Public URL returned by GetBlockEngineEndpoints (must be reachable from validators).
    #[arg(long, default_value = "http://127.0.0.1:10000")]
    public_url: String,

    /// Solana JSON-RPC URL used for allowlist + latest blockhash.
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

    #[command(flatten)]
    tls: TlsArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let (challenge_ttl, access_ttl, refresh_ttl) = default_ttls();
    let tokens = Arc::new(TokenStore::new(challenge_ttl, access_ttl, refresh_ttl));
    tokens.spawn_purge_task(PURGE_INTERVAL);
    let leaders = LeaderScheduleCache::new(
        args.rpc_url.clone(),
        args.allow_any_validator,
        Duration::from_secs(args.leader_refresh_secs),
    );

    let auth = AuthServiceImpl::new(tokens.clone(), leaders, vec![Role::Validator]);
    let validator = BlockEngineValidatorImpl {
        tokens,
        public_url: args.public_url.clone(),
        rpc_url: args.rpc_url.clone(),
    };

    warn_if_bad_public_url(&args.public_url);
    info!(
        "Bundle Server: bundles_server listening on {} public_url={}",
        args.bind, args.public_url
    );
    server_builder(&args.tls, &args.public_url)?
        .add_service(AuthServiceServer::new(auth))
        .add_service(BlockEngineValidatorServer::new(validator))
        .serve(args.bind)
        .await
        .context("gRPC serve")?;
    Ok(())
}

struct BlockEngineValidatorImpl {
    tokens: Arc<TokenStore>,
    public_url: String,
    rpc_url: String,
}

#[tonic::async_trait]
impl BlockEngineValidator for BlockEngineValidatorImpl {
    type SubscribePacketsStream = ReceiverStream<Result<SubscribePacketsResponse, Status>>;
    type SubscribeBundlesStream = ReceiverStream<Result<SubscribeBundlesResponse, Status>>;

    async fn subscribe_packets(
        &self,
        request: Request<SubscribePacketsRequest>,
    ) -> Result<Response<Self::SubscribePacketsStream>, Status> {
        let ctx = require_bearer(&request, &self.tokens, Role::Validator)?;
        info!("Bundle Server: SubscribePackets from {}", ctx.pubkey);
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            loop {
                tick.tick().await;
                if tx.is_closed() {
                    break;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn subscribe_bundles(
        &self,
        request: Request<SubscribeBundlesRequest>,
    ) -> Result<Response<Self::SubscribeBundlesStream>, Status> {
        let ctx = require_bearer(&request, &self.tokens, Role::Validator)?;
        info!("Bundle Server: SubscribeBundles from {}", ctx.pubkey);
        let (tx, rx) = mpsc::channel(16);
        let rpc_url = self.rpc_url.clone();
        tokio::spawn(async move {
            // Dummy: periodically create a tip tx and push it as a BundleUuid.
            // Replace with your real orderflow.
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            loop {
                tick.tick().await;
                if tx.is_closed() {
                    break;
                }
                // make_dummy_bundle does a blocking RPC call; keep it off the async workers.
                let rpc = rpc_url.clone();
                let built = match tokio::task::spawn_blocking(move || make_dummy_bundle(&rpc)).await {
                    Ok(res) => res,
                    Err(err) => Err(anyhow::anyhow!("dummy bundle task panicked: {err}")),
                };
                match built {
                    Ok((bundle, sig)) => {
                        info!(
                            "Bundle Server: SubscribeBundles sending dummy tip tx signature={sig} uuid={}",
                            bundle.uuid
                        );
                        if tx
                            .send(Ok(SubscribeBundlesResponse {
                                bundles: vec![bundle],
                            }))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(err) => warn!("Bundle Server: dummy bundle failed: {err:#}"),
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_block_builder_fee_info(
        &self,
        request: Request<BlockBuilderFeeInfoRequest>,
    ) -> Result<Response<BlockBuilderFeeInfoResponse>, Status> {
        let _ctx = require_bearer(&request, &self.tokens, Role::Validator)?;
        Err(Status::not_found("block builder fee not configured"))
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
