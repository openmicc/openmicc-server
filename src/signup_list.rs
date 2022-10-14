use std::collections::HashSet;

use actix::prelude::*;
use actix::{Actor, Context};
use anyhow::Context as AnyhowContext;
use redis::{Client as RedisClient, Commands, Connection as RedisConnection};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::constants::{SIGNUP_LIST, SIGNUP_TOPIC};
use crate::redis_subscriber::{RedisSubscriber, RedisSubscriberMessage};
use crate::user_session::{SignupListMessage, UserSession};
use crate::utils::{LogError, MyAddr, SendAndCheckResult, WrapAddr};

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
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

impl Handler<RedisMessage> for SignupListActor {
    type Result = ();

    #[instrument(skip(self, ctx), name = "RedisMessageHandler")]
    fn handle(&mut self, msg: RedisMessage, ctx: &mut Self::Context) -> Self::Result {
        info!("handle redis message");
        match msg {
            RedisMessage::Update { topic, content } => {
                info!("signup received redis update.");
                info!("{}: {}", topic, content);

                // Should be, but just double checking
                if topic == SIGNUP_TOPIC {
                    let msg = SignupListMessage::New {
                        new: content.into(),
                    };
                    let users = self.users.clone();
                    for user in users {
                        self.send_and_check_result(ctx, user, msg.clone())
                    }
                } else {
                    warn!("got weird topic: {}", topic);
                }

                info!("signup dispatched messages");
            }
        }
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
//

/// Signup list API for users
pub mod user_api {
    use super::*;
    use actix::{Message, MessageResult};

    /// Subscribe to list updates
    #[derive(Debug, Message)]
    #[rtype(result = "()")]
    pub struct Subscribe(pub MyAddr<UserSession>);

    impl Handler<Subscribe> for SignupListActor {
        type Result = ();

        #[instrument(skip(self, _ctx))]
        fn handle(&mut self, msg: Subscribe, _ctx: &mut Self::Context) -> Self::Result {
            let addr = msg.0;
            let is_new = self.users.insert(addr.clone());
            if !is_new {
                warn!("UserSession w/ addr {:?} is already subscribed", addr);
            }
        }
    }

    /// Unsubscribe from list updates
    #[derive(Debug, Message)]
    #[rtype(result = "()")]
    pub struct Unsubscribe(pub MyAddr<UserSession>);

    impl Handler<Unsubscribe> for SignupListActor {
        type Result = ();

        #[instrument(skip(self, _ctx))]
        fn handle(&mut self, msg: Unsubscribe, _ctx: &mut Self::Context) -> Self::Result {
            let addr = msg.0;
            let was_present = self.users.remove(&addr);
            if !was_present {
                warn!("UserSession w/ addr {:?} was not subscribed", addr);
            }
        }
    }

    /// Add my name to the list
    #[derive(Debug, Message)]
    #[rtype(result = "anyhow::Result<()>")]
    pub struct SignUp(pub String);

    /// Get a snapshot of the current list
    #[derive(Debug, Message)]
    #[rtype(result = "anyhow::Result<SignupList>")]
    pub struct GetList;

    impl Handler<GetList> for SignupListActor {
        type Result = MessageResult<GetList>;

        #[instrument(skip(self, _ctx), name = "GetListHandler")]
        fn handle(&mut self, _msg: GetList, _ctx: &mut Self::Context) -> Self::Result {
            // Get the current signup list
            // let current_list = self.get_list()?;
            MessageResult(self.get_list())
        }
    }
}

pub struct SignupListActor {
    /// Resdis client
    client: RedisClient,
    /// Redis connection
    redis: RedisConnection,
    /// Users who are subscribed to the list
    users: HashSet<MyAddr<UserSession>>,
}

impl SignupListActor {
    #[instrument]
    fn try_new(client: RedisClient) -> anyhow::Result<Self> {
        let redis = client.get_connection()?;

        let new = Self {
            client,
            redis,
            users: Default::default(),
        };

        Ok(new)
    }

    #[instrument(skip(self))]
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

    #[instrument(skip(self))]
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

    #[instrument(skip_all)]
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Start signup list");

        // Register for updates & start subscriber
        let addr = ctx.address();

        let subscriber = RedisSubscriber::<Self>::new(self.client.clone(), SIGNUP_TOPIC);
        let subscriber_addr = subscriber.start();
        let register_msg = RedisSubscriberMessage::Register(addr.wrap());

        subscriber_addr
            .try_send(register_msg)
            .context("failed to register with subscriber")
            .log_err();

        info!("signup list actor start finished");
    }

    #[instrument(skip_all)]
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Stop signup list");
    }
}
