use actix::prelude::*;
use anyhow::Context as AnyhowContext;
use std::collections::HashSet;

use actix::{Actor, Context};
use redis::{Client as RedisClient, Commands, Connection as RedisConnection};
use serde::{Deserialize, Serialize};

use crate::constants::{SIGNUP_LIST, SIGNUP_TOPIC};
use crate::redis_subscriber::{RedisSubscriber, RedisSubscriberMessage};
use crate::user_session::{SignupListMessage, UserSession};
use crate::utils::send_or_log_err;

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
pub enum RedisMessage {
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

impl Handler<SubscribeToSignupList> for SignupListActor {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: SubscribeToSignupList, _ctx: &mut Self::Context) -> Self::Result {
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

pub fn start_signup_list(redis: RedisClient) -> anyhow::Result<Addr<SignupListActor>> {
    // TODO: actors should be able to reconnect to redis
    // (or just die & restart would be fine)
    // ((but then how do others get the new address?))
    let actor = SignupListActor::try_new(redis)?;
    let addr = actor.start();
    Ok(addr)
}

// Signup-list messages
//

/// Sent from UserSession to SignupListActor
/// to receive updates about the signup list.
/// A snapshot of the current signup list is returned.
#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<()>")]
pub struct SubscribeToSignupList(pub Addr<UserSession>);

pub struct SignupListActor {
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
pub struct Signup(String);

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
pub struct SignupList(Vec<Signup>);

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

        println!("signup list actor start finished");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("Stop signup list");
    }
}
