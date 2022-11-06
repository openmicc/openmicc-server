use anyhow::Context;
use clap::Parser;
use mediasoup::{
    router::RouterOptions,
    rtp_parameters::RtpCodecCapability,
    worker::{Worker, WorkerLogLevel, WorkerLogTag, WorkerSettings},
    worker_manager::WorkerManager,
};
use openmicc_server::{
    greeter::{start_greeter, AddressBook, GreeterInfo},
    http_server::{run_http_server, AppData},
    signup_list::start_signup_list,
    stage::start_stage,
};
use redis::Client as RedisClient;
use tracing::info;

#[derive(Parser)]
struct Opts {
    /// Port to serve on
    #[clap(default_value_t = 3050)]
    port: u16,

    /// Redis connection string
    #[clap(default_value = "redis://127.0.0.1:6379", env = "REDIS_URL")]
    redis: String,
}

fn init_tracing() {
    tracing_subscriber::fmt().pretty().init();
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    info!("Starting.");

    // Initialize
    init_tracing();
    let opts = Opts::parse();

    // Start signup list
    info!("Connecting to redis at {}", opts.redis);
    let redis = RedisClient::open(opts.redis).context("creating redis client")?;
    let signup_list = start_signup_list(redis).context("starting signup list")?;

    let worker_manager = WorkerManager::new();
    let worker = create_worker(&worker_manager)
        .await
        .context("creating worker")?;

    let codecs = media_codecs();
    let router_options = RouterOptions::new(codecs);
    let router = worker
        .create_router(router_options)
        .await
        .context("creating router")?;

    let router_rtp_capabilities = router.rtp_capabilities().clone();

    info!("Creating stage");
    let stage = start_stage(router).await.context("creating stage")?;
    info!("Stage created");

    // Create greeter
    let addrs = AddressBook { signup_list, stage };
    let info = GreeterInfo {
        router_rtp_capabilities,
    };
    let greeter_addr = start_greeter(addrs, info);

    // Run HTTP server
    let app_data = AppData {
        greeter_addr,
        worker_manager,
    };
    run_http_server(opts.port, app_data)
        .await
        .context("running http server")?;

    Ok(())
}

async fn create_worker(manager: &WorkerManager) -> anyhow::Result<Worker> {
    let mut settings = WorkerSettings::default();
    settings.log_level = WorkerLogLevel::Debug;
    settings.log_tags = vec![
        WorkerLogTag::Info,
        WorkerLogTag::Ice,
        WorkerLogTag::Dtls,
        WorkerLogTag::Rtp,
        WorkerLogTag::Srtp,
        WorkerLogTag::Rtcp,
        WorkerLogTag::Rtx,
        WorkerLogTag::Bwe,
        WorkerLogTag::Score,
        WorkerLogTag::Simulcast,
        WorkerLogTag::Svc,
        WorkerLogTag::Sctp,
        WorkerLogTag::Message,
    ];

    let worker = manager
        .create_worker(settings)
        .await
        .context("create_worker")?;

    Ok(worker)
}

fn media_codecs() -> Vec<RtpCodecCapability> {
    use mediasoup::rtp_parameters::{
        MimeTypeAudio, MimeTypeVideo, RtcpFeedback, RtpCodecParametersParameters,
    };
    use std::num::{NonZeroU32, NonZeroU8};

    vec![
        RtpCodecCapability::Audio {
            mime_type: MimeTypeAudio::Opus,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(48000).unwrap(),
            channels: NonZeroU8::new(2).unwrap(),
            parameters: RtpCodecParametersParameters::from([("useinbandfec", 1_u32.into())]),
            rtcp_feedback: vec![RtcpFeedback::TransportCc],
        },
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::Vp8,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(90000).unwrap(),
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        },
    ]
}
