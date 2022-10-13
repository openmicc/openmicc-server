// use mediasoup::{
//     worker::{WorkerLogLevel, WorkerSettings},
//     worker_manager::WorkerManager,
// };

use std::collections::HashSet;
use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

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
use anyhow::{bail, Context as AnyhowContext};
use clap::Parser;
use futures::{Future, StreamExt};
use redis::aio::PubSub;
use redis::{
    aio::Connection as RedisAsyncConnection, AsyncCommands, Client as RedisClient, Commands,
    Connection as RedisConnection, ControlFlow, PubSubCommands,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

// Redis keys/topics
const SIGNUP_TOPIC: &str = "new_signups";
const SIGNUP_LIST: &str = "signup_list";

async fn old_main() -> std::io::Result<()> {
    // let flags = xflags::parse_or_exit! {
    //     /// Port to serve on
    //     required port: u16
    // };

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

    // println!("Serving on port {}.", &flags.port);
    // warp::serve(hello).run(([0, 0, 0, 0], flags.port)).await;
    println!("Done.");

    Ok(())
}

struct RedisSubscriber<A: Actor> {
    client: RedisClient,
    topic: String,
    addrs: HashSet<Addr<A>>,
}

impl<A> RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    pub fn new<S: ToString>(client: RedisClient, topic: S) -> Self {
        Self {
            client,
            topic: topic.to_string(),
            addrs: Default::default(),
        }
    }

    /// Register an actor as a forwarding address
    fn register(&mut self, addr: Addr<A>) {
        self.addrs.insert(addr);
    }

    /// Unregister an actor as a forwarding address
    fn unregister(&mut self, addr: Addr<A>) -> bool {
        self.addrs.remove(&addr)
    }

    fn broadcast(&self, msg: RedisMessage) {
        println!("Sending to {} addrs", self.addrs.len());

        for addr in &self.addrs {
            send_or_log_err(addr, msg.clone());
        }

        println!("sent payload");
    }
}

impl<A> Actor for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("starting RedisSubscriber");

        let topic = self.topic.clone();
        let client = self.client.clone();

        let block = async move {
            let redis = client.get_async_connection().await?;
            let mut pubsub = redis.into_pubsub();
            pubsub.subscribe(&topic).await?;

            let stream = pubsub.into_on_message();
            let mapped = stream.filter_map(|msg| async {
                let res = RedisMessage::try_from(msg);
                res.map_err(|err| {
                    eprintln!("Error converting RM: {:?}", err);
                })
                .ok()
            });

            Ok(mapped)
        };

        let logged = block.into_actor(self).map(map_stream);

        ctx.spawn(logged);
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        println!("stopping RedisSubscriber");
    }
}

fn map_stream<A, S>(res: anyhow::Result<S>, act: &mut A, ctx: &mut A::Context)
where
    A: Actor + StreamHandler<RedisMessage>,
    A::Context: AsyncContext<A>,
    // A::Context: ToEnvelope<A, RedisMessage>,
    S: Stream<Item = RedisMessage> + 'static,
{
    println!("mapping stream");
    match res {
        Ok(stream) => {
            ctx.add_stream(stream);
        }
        Err(err) => {
            eprintln!("ERROR: {:?}", err);
        }
    }
}

impl<A> Handler<RedisSubscriberMessage<A>> for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    type Result = ();
    fn handle(&mut self, msg: RedisSubscriberMessage<A>, _ctx: &mut Self::Context) {
        match msg {
            RedisSubscriberMessage::Register(addr) => {
                self.register(addr);
            }
            RedisSubscriberMessage::Unregister(addr) => {
                self.unregister(addr);
            }
        }
    }
}

impl<A> StreamHandler<RedisMessage> for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    fn handle(&mut self, msg: RedisMessage, ctx: &mut Self::Context) {
        println!("RS got RedisMessage {:?}", msg);
        self.broadcast(msg);
    }
}

struct UserSession {
    signup_list_addr: Addr<SignupListActor>,
}

impl Actor for UserSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("started");

        let addr = ctx.address();

        // Subscribe to redis updates
        let subscribe_msg = SubscribeToSignupList(addr);
        send_or_log_err(&self.signup_list_addr, subscribe_msg);

        println!("started done");
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> actix::Running {
        println!("stopping");
        actix::Running::Stop
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        println!("stopped");
        // self.signup_feed.unregister(ctx.address());
    }
}

struct SignupListActor {
    /// Resdis client
    client: RedisClient,
    /// Redis connection
    redis: RedisConnection,
    /// Users who are subscribed to the list
    users: HashSet<Addr<UserSession>>,
}

impl SignupListActor {
    fn try_new(client: RedisClient) -> anyhow::Result<Self> {
        let redis = client.get_connection()?;

        let new = Self {
            client,
            redis,
            users: Default::default(),
        };

        Ok(new)
    }

    fn publish_signup(&mut self, signup: Signup) -> anyhow::Result<()> {
        let signup_string = signup.to_string();

        let tx = || -> anyhow::Result<()> {
            // Publish
            self.redis.lpush(SIGNUP_LIST, signup_string.clone())?;
            self.redis.publish(SIGNUP_TOPIC, signup_string)?;

            Ok(())
        };
        if let Ok(_) = tx() {
            redis::cmd("EXEC").query(&mut self.redis)?;
        } else {
            redis::cmd("DISCARD").query(&mut self.redis)?;
        }

        Ok(())
    }

    fn get_list(&mut self) -> anyhow::Result<SignupList> {
        let list: Vec<String> = self.redis.lrange(SIGNUP_LIST, 0, -1)?;

        Ok(list.into())
    }
}

/// An entry on the signup list.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Signup(String);

impl ToString for Signup {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl From<String> for Signup {
    fn from(val: String) -> Self {
        Self(val)
    }
}

/// A snapshot of the whole signup list.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct SignupList(Vec<Signup>);

impl<E, L> From<L> for SignupList
where
    E: Into<Signup>,
    L: IntoIterator<Item = E>,
{
    fn from(vals: L) -> Self {
        Self(vals.into_iter().map(Into::into).collect())
    }
}

impl Actor for SignupListActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("Start signup list");

        // Register for updates & start subscriber
        let addr = ctx.address();

        let subscriber = RedisSubscriber::<Self>::new(self.client.clone(), SIGNUP_TOPIC);
        let subscriber_addr = subscriber.start();
        let register_msg = RedisSubscriberMessage::Register(addr);

        subscriber_addr
            .try_send(register_msg)
            .context("failed to register with subscriber")
            .map_err(|err| eprintln!("ERR: {:?}", err))
            .ok();

        // self.subscriber.register(addr);
        // let cloned_subscriber = self.subscriber.clone();
        println!("spawning listener");
        println!("listener spawned");

        println!("signup list actor start finished");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        println!("Stop signup list");
        let addr = ctx.address();
        println!("unregister");
        // self.subscriber.unregister(addr);
        println!("stopped")
    }
}

/// Sent from client to sever
#[derive(Debug, Deserialize, Message)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum ClientMessage {
    /// Sign me up.
    SignMeUp(Signup),
    // TODO: ImReady (I'm ready to perform)
}

/// Sent from server to client
#[derive(Serialize, Message)]
#[serde(tag = "action")]
#[serde(rename = "camelCase")]
#[rtype(result = "()")]
enum ServerMessage {
    // TODO: Pop from list (after finishing)
    // TODO: Remove person from list (dropped out early)
    /// The whole current sign-up list.
    SignupList(SignupList),
    /// A notification of a new sign-up.
    NewSignup(Signup),
    // TODO: AreYouReady (ready to perform?)
}

// Signup-list messages
//

/// Sent from UserSession to SignupListActor
/// to receive updates about the signup list.
/// A snapshot of the current signup list is returned.
#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<()>")]
struct SubscribeToSignupList(pub Addr<UserSession>);

/// A message from the SignupListActor to the UserSession
/// with information about the signup list
#[derive(Clone, Debug, Message, Serialize)]
#[rtype(result = "anyhow::Result<()>")]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
enum SignupListMessage {
    // TODO Probably better to have a single ServerMessage type.
    All { list: SignupList },
    New { new: Signup },
}

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
enum RedisSubscriberMessage<A: Actor> {
    Register(Addr<A>),
    Unregister(Addr<A>),
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

impl Handler<RedisMessage> for SignupListActor {
    type Result = ();

    fn handle(&mut self, msg: RedisMessage, _ctx: &mut Self::Context) -> Self::Result {
        println!("handle redis message");
        match msg {
            RedisMessage::Update { topic, content } => {
                println!("signup received redis update.");
                println!("{}: {}", topic, content);

                if topic == SIGNUP_TOPIC {
                    let msg = SignupListMessage::New {
                        new: content.into(),
                    };
                    for addr in &self.users {
                        send_or_log_err(&addr, msg.clone());
                    }
                }

                println!("signup dispatched messges");
            }
        }
    }
}

impl Handler<SignupListMessage> for UserSession {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: SignupListMessage, ctx: &mut Self::Context) -> Self::Result {
        println!("User got signup list message: {:?}", &msg);
        // forward the message over WebSockets to the client, encoded as JSON
        let serialized = serde_json::to_string(&msg).context("serializing SignupListMessage")?;
        println!("Serialized...");
        ctx.text(serialized);

        println!("forwarded signup list message over WS");

        Ok(())
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
                    send_or_log_err(&ctx.address(), message)
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
            ClientMessage::SignMeUp(signup) => {
                // TODO send message to signup list
                todo!()
            }
        }
    }
}

impl Handler<SubscribeToSignupList> for SignupListActor {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: SubscribeToSignupList, ctx: &mut Self::Context) -> Self::Result {
        println!("got signup list subscribe request");
        // Get the current signup list
        let current_list = self.get_list()?;

        println!("got current list: {:?}", &current_list);

        // Susbcribe the new user
        let SubscribeToSignupList(addr) = msg;
        self.users.insert(addr.clone());

        println!("subscribed user");

        // Send the current list to the new user
        let list_msg = SignupListMessage::All { list: current_list };
        send_or_log_err(&addr, list_msg);

        println!("sent list to user");

        Ok(())
    }
}

async fn send_or_log_err_inner<A, M>(addr: Addr<A>, msg: M)
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

fn send_or_log_err<A, M>(addr: &Addr<A>, msg: M)
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

/// Function that receives HTTP request on WebSocket route and upgrades it to WebSocket connection.
///
/// See https://actix.rs/docs/websockets/ for official `actix-web` documentation.
async fn ws_index(
    request: HttpRequest,
    data: Data<AppData>,
    stream: Payload,
) -> Result<HttpResponse, ActixError> {
    println!("ws_index");
    let actor = UserSession {
        signup_list_addr: data.signup_list_addr.clone(),
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
    // subscriber: Arc<RedisSubscriber<UserSession>>,
    signup_list_addr: Addr<SignupListActor>,
}

// fn create_redis_subscriber<S: ToString>(
//     redis: RedisClient,
//     topic: S,
// ) -> Arc<RedisSubscriber<SignupListActor>> {
//     Arc::new(RedisSubscriber::new(redis, topic))
// }

async fn run_http_server(port: u16, app_data: AppData) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
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
    .await?;

    Ok(())
}

fn start_signup_list(redis: RedisClient) -> anyhow::Result<Addr<SignupListActor>> {
    // TODO: actors should be able to reconnect to redis
    // (or just die & restart would be fine)
    // ((but then how do others get the new address?))
    let actor = SignupListActor::try_new(redis)?;
    let addr = actor.start();
    Ok(addr)
}

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
