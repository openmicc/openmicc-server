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

pub async fn send_or_log_err_inner<A, M>(addr: Addr<A>, msg: M)
where
    A: Actor,
    A::Context: ToEnvelope<A, M>,
    M: std::fmt::Debug + Message + Send + 'static,
    M::Result: std::fmt::Debug + Send,
    A: Handler<M>,
{
    println!("SEND MESSAGE {:?}", &msg);
    match addr.send(msg).await {
        Ok(res) => println!("Send result {:?}", res),
        Err(err) => eprintln!("ERROR 1: {:?}", err),
    }
}

pub fn send_or_log_err<A, M>(addr: &Addr<A>, msg: M)
where
    A: Actor,
    A::Context: ToEnvelope<A, M>,
    M: std::fmt::Debug + Message + Send + 'static,
    M::Result: std::fmt::Debug + Send,
    A: Handler<M>,
{
    let addr = addr.clone();
    tokio::spawn(send_or_log_err_inner(addr, msg));
}
