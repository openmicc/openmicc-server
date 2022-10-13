use std::collections::HashSet;
use std::convert::TryFrom;

use actix::dev::ToEnvelope;
use actix::prelude::*;

use actix::{Actor, ActorFutureExt, Context, StreamHandler};
use actix_web::error::Error as ActixError;
use actix_web::App;
use actix_web::{
    web::{get, Data, Payload},
    HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;
use anyhow::Context as AnyhowContext;
use clap::Parser;
use futures::StreamExt;
use redis::{Client as RedisClient, Commands, Connection as RedisConnection};
use serde::{Deserialize, Serialize};

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
    let signup_list_addr = start_signup_list(redis)?;

    // Run HTTP server
    let app_data = AppData { signup_list_addr };
    run_http_server(opts.port, app_data).await?;

    Ok(())
}
