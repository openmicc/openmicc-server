use clap::Parser;
use openmicc_server::{
    greeter::{start_greeter, AddressBook},
    http_server::{run_http_server, AppData},
    signup_list::start_signup_list,
};
use redis::Client as RedisClient;

#[derive(Parser)]
struct Opts {
    /// Port to serve on
    port: u16,

    /// Redis connection string
    #[arg(default_value = "redis://127.0.0.1:6379")]
    redis: String,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // Initialize
    env_logger::init();
    let opts = Opts::parse();

    // Start signup list
    let redis = RedisClient::open(opts.redis)?;
    let signup_list = start_signup_list(redis)?;

    let addrs = AddressBook { signup_list };
    let greeter_addr = start_greeter(addrs);

    // Run HTTP server
    let app_data = AppData { greeter_addr };
    run_http_server(opts.port, app_data).await?;

    Ok(())
}
