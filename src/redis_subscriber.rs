use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::Debug;

use actix::dev::ToEnvelope;
use actix::prelude::*;

use actix::{Actor, ActorFutureExt, Context, StreamHandler};
use futures::StreamExt;
use redis::Client as RedisClient;
use tracing::{info, info_span, instrument, warn};
use tracing_actix::ActorInstrument;

use crate::utils::{LogOk, MyAddr, SendAndCheckResult};

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub enum RedisSubscriberMessage<A: Actor> {
    Register(MyAddr<A>),
    Unregister(MyAddr<A>),
}

impl<A: Actor> Debug for RedisSubscriberMessage<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Register(arg0) => f.debug_tuple("Register").field(arg0).finish(),
            Self::Unregister(arg0) => f.debug_tuple("Unregister").field(arg0).finish(),
        }
    }
}

#[derive(Clone, Debug, Message)]
#[rtype(result = "anyhow::Result<()>")]
pub enum RedisMessage {
    Update { topic: String, content: String },
}

impl TryFrom<redis::Msg> for RedisMessage {
    type Error = anyhow::Error;

    #[instrument]
    fn try_from(msg: redis::Msg) -> Result<Self, Self::Error> {
        let topic = msg.get_channel_name().to_string();
        let content: String = msg.get_payload()?;

        let converted = Self::Update { topic, content };

        Ok(converted)
    }
}

impl<A> StreamHandler<RedisMessage> for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    #[instrument(skip(ctx), name = "RedisMessageStreamHandler")]
    fn handle(&mut self, msg: RedisMessage, ctx: &mut Self::Context) {
        info!("RS got RedisMessage {:?}", msg);
        self.broadcast(ctx, msg);
    }
}

pub struct RedisSubscriber<A: Actor> {
    client: RedisClient,
    topic: String,
    addrs: HashSet<MyAddr<A>>,
}

impl<A: Actor> Debug for RedisSubscriber<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisSubscriber")
            .field("topic", &self.topic)
            .finish()
    }
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
    #[instrument]
    fn register(&mut self, addr: MyAddr<A>) {
        self.addrs.insert(addr);
    }

    /// Unregister an actor as a forwarding address
    #[instrument]
    fn unregister(&mut self, addr: MyAddr<A>) -> bool {
        self.addrs.remove(&addr)
    }

    #[instrument(skip(ctx))]
    fn broadcast(&mut self, ctx: &mut <Self as Actor>::Context, msg: RedisMessage) {
        info!("Sending to {} addrs", self.addrs.len());

        let addrs = self.addrs.clone();

        for addr in addrs {
            self.send_and_check_result(ctx, addr, msg.clone());
        }

        info!("sent payload");
    }
}

impl<A> Actor for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("starting RedisSubscriber");

        let topic = self.topic.clone();
        let client = self.client.clone();

        let stream_res_fut = async move {
            let redis = client.get_async_connection().await?;
            let mut pubsub = redis.into_pubsub();
            pubsub.subscribe(&topic).await?;
            // Stream of redis::Msg
            let stream = pubsub.into_on_message();
            // Stream of Option<RedisMessage>
            let mapped =
                stream.filter_map(|msg| async { RedisMessage::try_from(msg).ok_log_err() });
            Ok::<_, anyhow::Error>(mapped)
        };

        // Once the stream is created,
        // add it to the current context.
        let span = info_span!("Following up on RedisMessage stream creation");
        let do_later =
            stream_res_fut
                .into_actor(self)
                .actor_instrument(span)
                .map(|stream_res, _act, ctx| {
                    stream_res.ok_log_err().map(|stream| ctx.add_stream(stream));
                });

        ctx.spawn(do_later);
    }

    #[instrument(skip(_ctx))]
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("stopping RedisSubscriber");
    }
}

impl<A> Handler<RedisSubscriberMessage<A>> for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    type Result = ();
    #[instrument(skip(_ctx), name = "RedisSubscriberMessageHandler")]
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
