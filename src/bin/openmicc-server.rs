use anyhow::Context;
use clap::Parser;
use openmicc_server::{
    greeter::{start_greeter, AddressBook},
    http_server::{run_http_server, AppData},
    signup_list::start_signup_list,
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
    let addr = signup_list.wrap();

    let addrs = AddressBook { signup_list: addr };
    let greeter_addr = start_greeter(addrs);

    // Run HTTP server
    let app_data = AppData { greeter_addr };
    run_http_server(opts.port, app_data).await?;

    Ok(())
}
