use std::collections::HashSet;
use std::convert::TryFrom;

use actix::dev::ToEnvelope;
use actix::prelude::*;

use actix::{Actor, ActorFutureExt, Context, StreamHandler};
use futures::StreamExt;
use redis::Client as RedisClient;

use crate::signup_list::RedisMessage;
use crate::utils::send_or_log_err;

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
pub enum RedisSubscriberMessage<A: Actor> {
    Register(Addr<A>),
    Unregister(Addr<A>),
}

impl<A> StreamHandler<RedisMessage> for RedisSubscriber<A>
where
    A: Actor + Handler<RedisMessage>,
    A::Context: ToEnvelope<A, RedisMessage>,
{
    fn handle(&mut self, msg: RedisMessage, _ctx: &mut Self::Context) {
        println!("RS got RedisMessage {:?}", msg);
        self.broadcast(msg);
    }
}

pub struct RedisSubscriber<A: Actor> {
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

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("stopping RedisSubscriber");
    }
}

fn map_stream<A, S>(res: anyhow::Result<S>, _act: &mut A, ctx: &mut A::Context)
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