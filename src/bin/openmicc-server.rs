use anyhow::Context;
use clap::Parser;
use mediasoup::{
    worker::{Worker, WorkerLogLevel, WorkerLogTag, WorkerSettings},
    worker_manager::WorkerManager,
};
use openmicc_server::{
    greeter::{start_greeter, AddressBook},
    http_server::{run_http_server, AppData},
    signup_list::start_signup_list,
    stage::{start_stage, Stage},
    utils::WrapAddr,
};
use redis::Client as RedisClient;
use tracing::info;

#[derive(Parser)]
struct Opts {
    /// Port to serve on
    #[arg(default_value_t = 3050)]
    port: u16,

    /// Redis connection string
    #[arg(default_value = "redis://127.0.0.1:6379", env = "REDIS_URL")]
    redis: String,
}

fn init_tracing() {
    tracing_subscriber::fmt().pretty().init();
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
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

    info!("Creating stage");
    let stage = start_stage(&worker).await.context("creating stage")?;
    info!("Stage created");

    // Create greeter
    let addrs = AddressBook { signup_list, stage };
    let greeter_addr = start_greeter(addrs);

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
