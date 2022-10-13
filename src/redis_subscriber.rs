use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::Debug;

use actix::dev::ToEnvelope;
use actix::prelude::*;

use actix::{Actor, ActorFutureExt, Context, StreamHandler};
use futures::StreamExt;
use redis::Client as RedisClient;
use tracing::{info, instrument};

use crate::signup_list::RedisMessage;
use crate::utils::{send_or_log_err, LogOk};

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub enum RedisSubscriberMessage<A: Actor> {
    Register(Addr<A>),
    Unregister(Addr<A>),
}

impl<A: Actor> Debug for RedisSubscriberMessage<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Register(arg0) => f.debug_tuple("Register").field(arg0).finish(),
            Self::Unregister(arg0) => f.debug_tuple("Unregister").field(arg0).finish(),
        }
    }
}

impl<A> StreamHandler<RedisMessage> for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    #[instrument(skip(_ctx), name = "RedisMessageStreamHandler")]
    fn handle(&mut self, msg: RedisMessage, _ctx: &mut Self::Context) {
        info!("RS got RedisMessage {:?}", msg);
        self.broadcast(msg);
    }
}

pub struct RedisSubscriber<A: Actor> {
    client: RedisClient,
    topic: String,
    addrs: HashSet<Addr<A>>,
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
    fn register(&mut self, addr: Addr<A>) {
        self.addrs.insert(addr);
    }

    /// Unregister an actor as a forwarding address
    #[instrument]
    fn unregister(&mut self, addr: Addr<A>) -> bool {
        self.addrs.remove(&addr)
    }

    #[instrument]
    fn broadcast(&self, msg: RedisMessage) {
        info!("Sending to {} addrs", self.addrs.len());

        for addr in &self.addrs {
            send_or_log_err(addr, msg.clone());
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

    #[instrument(skip(ctx))]
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("starting RedisSubscriber");

        let topic = self.topic.clone();
        let client = self.client.clone();

        let block = async move {
            let redis = client.get_async_connection().await?;
            let mut pubsub = redis.into_pubsub();
            pubsub.subscribe(&topic).await?;

            let stream = pubsub.into_on_message();
            let mapped =
                stream.filter_map(|msg| async { RedisMessage::try_from(msg).ok_log_err() });

            Ok(mapped)
        };

        let logged = block.into_actor(self).map(map_stream);

        ctx.spawn(logged);
    }

    #[instrument(skip(_ctx))]
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("stopping RedisSubscriber");
    }
}

fn map_stream<A, S>(res: anyhow::Result<S>, _act: &mut A, ctx: &mut A::Context)
where
    A: Actor + StreamHandler<RedisMessage>,
    A::Context: AsyncContext<A>,
    // A::Context: ToEnvelope<A, RedisMessage>,
    S: Stream<Item = RedisMessage> + 'static,
{
    res.ok_log_err().map(|stream| ctx.add_stream(stream));
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
