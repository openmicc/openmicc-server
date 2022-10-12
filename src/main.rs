// use mediasoup::{
//     worker::{WorkerLogLevel, WorkerSettings},
//     worker_manager::WorkerManager,
// };

use std::collections::HashSet;
use std::convert::TryFrom;
use std::sync::{Arc, RwLock};

use actix::dev::ToEnvelope;
use actix::prelude::*;

use actix::{Actor, StreamHandler};
use actix_web::error::Error as ActixError;
use actix_web::{
    web::{get, Data, Payload},
    HttpRequest, HttpResponse, HttpServer,
};
use actix_web::{App, ResponseError};
use actix_web_actors::ws;
use anyhow::Context;
use redis::{Client as RedisClient, Connection as RedisConnection};
use serde::Deserialize;

async fn old_main() -> std::io::Result<()> {
    let flags = xflags::parse_or_exit! {
        /// Port to serve on
        required port: u16
    };

    println!("Starting.");
    // let manager = WorkerManager::new();

    // let worker_settings = WorkerSettings {
    //     log_level: WorkerLogLevel::Debug,
    //     log_tags: vec![],
    //     rtc_ports_range: todo!(),
    //     dtls_files: todo!(),
    //     thread_initializer: todo!(),
    //     app_data: todo!(),
    // };
    // let worker = manager.create_worker(worker_settings);

    // GET /hello/warp => 200 OK with body "Hello, warp!"
    // let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!\n", name));

    println!("Serving on port {}.", &flags.port);
    // warp::serve(hello).run(([0, 0, 0, 0], flags.port)).await;
    println!("Done.");

    Ok(())
}

struct RedisSubscriber<A: Actor> {
    redis: RedisClient,
    channel: String,
    addrs: RwLock<HashSet<Addr<A>>>,
}

impl<A> RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    fn new<S: ToString>(redis: RedisClient, channel: S) -> Self {
        Self {
            redis,
            channel: channel.to_string(),
            addrs: Default::default(),
        }
    }

    /// Register an actor as a forwarding address
    fn register(&self, addr: Addr<A>) {
        let mut addrs = self.addrs.write().expect("write lock poisoned");
        addrs.insert(addr);
    }

    /// Unregister an actor as a forwarding address
    fn unregister(&self, addr: Addr<A>) -> bool {
        let mut addrs = self.addrs.write().expect("write lock poisoned");

        addrs.remove(&addr)
    }

    fn listen(&self) -> anyhow::Result<()> {
        let mut conn = self.redis.get_connection().context("connecting to redis")?;
        let mut pubsub = conn.as_pubsub();

        pubsub.subscribe(&self.channel)?;
        println!("subscribed");

        loop {
            println!("subscribe loop");
            let msg = pubsub.get_message()?;
            println!("OK");

            let redis_msg: RedisMessage = msg.try_into()?;

            let addrs = self.addrs.read().expect("read lock poisoned");

            println!("Sending to {} addrs", addrs.len());

            for addr in &*addrs {
                // TODO: Use addr.send?
                addr.do_send(redis_msg.clone());
            }

            // addr.do_send(payload);
            println!("sent payload");
        }
    }
}

struct UserSession {
    redis: RedisConnection,
    subscriber: Arc<RedisSubscriber<UserSession>>,
}

impl Actor for UserSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("started");

        // let channel = "the_news";
        let addr = ctx.address();

        // let redis = self.redis.clone();

        // Subscribe to redis updates
        self.subscriber.register(addr);

        // ctx.notify(RedisMessage::Update {
        //     topic: "TOPIC".to_string(),
        //     content: "THIS IS A TEST".to_string(),
        // });

        // // Listen for updates in a separate thread
        // std::thread::spawn(|| {
        //     subscribe::<UserSession, RedisMessage>(redis, channel, addr).log_expect("redis subscribe")
        // });

        // ctx.notify(RedisMessage::Update {
        //     topic: "TOPIC 1".to_string(),
        //     content: "THIS IS Another TEST".to_string(),
        // });

        println!("spawning");
        // tokio::spawn(fut);

        // ctx.notify(RedisMessage::Update {
        //     topic: "TOPIC 2".to_string(),
        //     content: "THIS IS AnotherNother TEST".to_string(),
        // });

        // ctx.spawn(fut.into_actor(self));
        // let mut pubsub = self.redis.as_pubsub();
        // pubsub.subscribe("the_news").expect("can't subscribe");
        //
        println!("started done");
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> actix::Running {
        println!("stopping");
        actix::Running::Stop
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        println!("stopped");
        self.subscriber.unregister(ctx.address());
    }
}

#[derive(Deserialize, Message)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum ClientMessage {
    Hello,
}

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
enum RedisMessage {
    Update { topic: String, content: String },
}

impl TryFrom<redis::Msg> for RedisMessage {
    type Error = anyhow::Error;

    fn try_from(msg: redis::Msg) -> Result<Self, Self::Error> {
        let topic = msg.get_channel_name().to_string();
        let content: String = msg.get_payload()?;

        let converted = Self::Update { topic, content };

        Ok(converted)
    }
}

// fn subscribe<A: Actor, M: Message + FromRedisValue + Send + 'static>(
//     redis: RedisClient,
//     channel: &str,
//     addr: Addr<A>,
// ) -> anyhow::Result<()>
// where
//     A: Handler<M>,
//     M::Result: Send,
//     M: std::fmt::Debug,
//     A::Context: ToEnvelope<A, M>,
// {
//     println!("subscribe");
//     let mut conn = redis.get_connection().context("connecting to redis")?;
//     let mut pubsub = conn.as_pubsub();

//     pubsub.subscribe(channel)?;
//     println!("subscribed");

//     loop {
//         println!("subscribe loop");
//         let msg = pubsub.get_message()?;
//         println!("OK");
//         let payload: M = msg.get_payload()?;

//         // TODO: Use addr.send?
//         println!("sending payload: {:?}", payload);
//         addr.do_send(payload);
//         // addr.do_send(payload);
//         println!("sent payload");
//     }
// }

// async fn run_or_eprintln<F: futures::Future>(fut: F)

impl Handler<RedisMessage> for UserSession {
    type Result = ();

    fn handle(&mut self, msg: RedisMessage, _ctx: &mut Self::Context) -> Self::Result {
        println!("handle redis message");
        match msg {
            RedisMessage::Update { topic, content } => {
                println!("Received redis update.");
                println!("{}: {}", topic, content);
            }
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for UserSession {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(message) => {
                    // Parse JSON into an enum and just send it back to the actor to be processed
                    // by another handler below, it is much more convenient to just parse it in one
                    // place and have typed data structure everywhere else
                    ctx.address().do_send(message);
                }
                Err(error) => {
                    eprintln!("Failed to parse client message: {}\n{}", error, text);
                }
            },
            Ok(ws::Message::Binary(bin)) => {
                eprintln!("Unexpected binary message: {:?}", bin);
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

impl Handler<ClientMessage> for UserSession {
    type Result = ();

    fn handle(&mut self, msg: ClientMessage, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ClientMessage::Hello => {
                println!("Got client hello!");
            }
        }
    }
}

/// Function that receives HTTP request on WebSocket route and upgrades it to WebSocket connection.
///
/// See https://actix.rs/docs/websockets/ for official `actix-web` documentation.
async fn ws_index(
    request: HttpRequest,
    data: Data<AppData>,
    stream: Payload,
) -> Result<HttpResponse, ActixError> {
    println!("ws_index");
    let redis_server_addr = "redis://127.0.0.1:6379";
    let redis_client = RedisClient::open(redis_server_addr).expect("failed to create redis client");
    let redis_conn = redis_client
        .get_connection()
        .map_err(|err| actix_web::error::ErrorServiceUnavailable(err))?;

    let actor = UserSession {
        redis: redis_conn,
        subscriber: data.subscriber.clone(),
    };
    ws::start(actor, &request, stream)

    // match EchoConnection::new(&worker_manager).await {
    //     Ok(echo_server) => {
    //         println!("Started echo server (upgrade WS)");
    //         ws::start(echo_server, &request, stream)
    //     }
    //     Err(error) => {
    //         eprintln!("{}", error);

    //         Ok(HttpResponse::InternalServerError().finish())
    //     }
    // }
}

async fn hello(
    _request: HttpRequest,
    _data: Data<AppData>,
    _stream: Payload,
) -> Result<HttpResponse, ActixError> {
    println!("Hello");
    let response = HttpResponse::Ok().body("Great job.".to_string());

    Ok(response)
}

#[derive(Clone)]
struct AppData {
    subscriber: Arc<RedisSubscriber<UserSession>>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let flags = xflags::parse_or_exit! {
        /// Port to serve on
        required port: u16
    };

    env_logger::init();

    let addr = format!("0.0.0.0:{}", flags.port);

    let redis_server_addr = "redis://127.0.0.1:6379";
    let redis_client = RedisClient::open(redis_server_addr).expect("failed to create redis client");

    let channel = "the_news";
    let redis_subscriber = Arc::new(RedisSubscriber::<UserSession>::new(redis_client, channel));

    let app_data = AppData {
        subscriber: redis_subscriber.clone(),
    };

    std::thread::spawn(move || redis_subscriber.clone().listen());

    println!("Running on {}", &addr);
    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(app_data.clone()))
            .route("/", get().to(ws_index))
            .route("/hello", get().to(hello))
    })
    .workers(2)
    .bind(addr)?
    .run()
    .await
}
