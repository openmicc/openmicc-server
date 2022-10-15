use std::collections::HashSet;

use actix::prelude::*;
use actix::{Actor, Context};
use anyhow::Context as AnyhowContext;
use redis::{Client as RedisClient, Commands, Connection as RedisConnection};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::constants::{SIGNUP_COUNTER, SIGNUP_LIST, SIGNUP_TOPIC};
use crate::redis_subscriber::{RedisMessage, RedisSubscriber, RedisSubscriberMessage};
use crate::signup_list_entry::{EntryAndReceipt, SignupId, SignupListEntry, SignupListEntryText};
use crate::signup_receipt::SignupReceipt;
use crate::user_session::{SignupListCounter, SignupListMessage, UserSession};
use crate::utils::{LogError, MyAddr, SendAndCheckResult, WrapAddr};

impl Handler<RedisMessage> for ListKeeper {
    type Result = anyhow::Result<()>;

    #[instrument(skip(self, ctx), name = "CountedRedisMessageHandler")]
    fn handle(&mut self, msg: RedisMessage, ctx: &mut Self::Context) -> Self::Result {
        info!("handle redis message");
        match msg {
            RedisMessage::Update {
                topic,
                content,
                // counter,
            } => {
                info!("signup received redis update.");
                info!("{}: {}", topic, content);

                let decoded: SignupListEntry =
                    serde_json::from_str(&content).context("decoding update content")?;

                let counter = self.update_counter.incr().clone();

                // Should be, but just double checking
                if topic == SIGNUP_TOPIC {
                    let msg = SignupListMessage::New {
                        new: decoded,
                        counter,
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

        Ok(())
    }
}

pub fn start_signup_list(redis: RedisClient) -> anyhow::Result<Addr<ListKeeper>> {
    // TODO: actors should be able to reconnect to redis
    // (or just die & restart would be fine)
    // ((but then how do others get the new address?))
    let actor = ListKeeper::try_new(redis)?;
    let addr = actor.start();
    Ok(addr)
}

// Signup-list messages
//
//

/// Signup list API for users
pub mod user_api {
    use crate::signup_list_entry::{IdAndReceipt, SignupListEntryText};

    use super::*;
    use actix::{Message, MessageResult};

    /// Subscribe to list updates
    #[derive(Debug, Message)]
    #[rtype(result = "()")]
    pub struct Subscribe(pub MyAddr<UserSession>);

    impl Handler<Subscribe> for ListKeeper {
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

    impl Handler<Unsubscribe> for ListKeeper {
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
    #[rtype(result = "anyhow::Result<IdAndReceipt>")]
    pub struct SignMeUp(pub SignupListEntryText);

    impl Handler<SignMeUp> for ListKeeper {
        type Result = anyhow::Result<IdAndReceipt>;

        fn handle(&mut self, msg: SignMeUp, _ctx: &mut Self::Context) -> Self::Result {
            let text = msg.0;
            let entry_and_receipt = self.attach_id_and_receipt_to_list_entry(text)?;
            let id = entry_and_receipt.entry.id.clone();
            let receipt = entry_and_receipt.receipt.clone();
            let id_and_receipt = IdAndReceipt { id, receipt };

            self.publish_signup(entry_and_receipt)?;

            Ok(id_and_receipt)
        }
    }

    /// Get a snapshot of the current list
    #[derive(Debug, Message)]
    #[rtype(result = "anyhow::Result<SignupList>")]
    pub struct GetList;

    impl Handler<GetList> for ListKeeper {
        type Result = MessageResult<GetList>;

        #[instrument(skip(self, _ctx), name = "GetListHandler")]
        fn handle(&mut self, _msg: GetList, _ctx: &mut Self::Context) -> Self::Result {
            // Get the current signup list
            // let current_list = self.get_list()?;
            MessageResult(self.get_list())
        }
    }
}

pub struct ListKeeper {
    /// Resdis client
    client: RedisClient,
    /// Redis connection
    redis: RedisConnection,
    /// Users who are subscribed to the list
    users: HashSet<MyAddr<UserSession>>,
    /// Enumerate updates so that clients
    /// can tell whether they've missed one.
    update_counter: SignupListCounter,
}

impl ListKeeper {
    #[instrument]
    fn try_new(client: RedisClient) -> anyhow::Result<Self> {
        let redis = client.get_connection()?;

        let new = Self {
            client,
            redis,
            users: Default::default(),
            update_counter: Default::default(),
        };

        Ok(new)
    }

    #[instrument(skip(self))]
    fn get_next_signup_id(&mut self) -> anyhow::Result<SignupId> {
        self.redis
            .incr(SIGNUP_COUNTER, 1)
            .context("incrementing signup counter")
    }

    #[instrument(skip(self))]
    fn attach_id_and_receipt_to_list_entry(
        &mut self,
        text: SignupListEntryText,
    ) -> anyhow::Result<EntryAndReceipt> {
        let id = self.get_next_signup_id()?;
        let entry = SignupListEntry { id, text };
        let receipt = SignupReceipt::gen();
        let entry_and_receipt = EntryAndReceipt { entry, receipt };

        Ok(entry_and_receipt)
    }

    #[instrument(skip(self))]
    fn publish_signup(&mut self, entry_and_receipt: EntryAndReceipt) -> anyhow::Result<()> {
        // TODO: Publish asynchronously?
        let encoded_entry =
            serde_json::to_string(&entry_and_receipt.entry).context("encoding signup entry")?;
        let encoded_entry_and_receipt = serde_json::to_string(&entry_and_receipt)
            .context("encoding signup entry and receipt")?;

        redis::cmd("MULTI").query(&mut self.redis)?;
        let tx = || -> anyhow::Result<()> {
            // Publish
            self.redis.rpush(SIGNUP_LIST, encoded_entry_and_receipt)?;
            self.redis.publish(SIGNUP_TOPIC, encoded_entry)?;

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
        let encoded_list: Vec<String> = self.redis.lrange(SIGNUP_LIST, 0, -1)?;

        let entries_res: Result<Vec<_>, _> = encoded_list
            .into_iter()
            .map(|encoded| serde_json::from_str(&encoded))
            .collect();

        let list = entries_res
            .context("decoding signup list entries")
            .map(SignupList)?;

        Ok(list)
    }
}

/// A snapshot of the whole signup list.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SignupList(Vec<SignupListEntry>);

impl<E, L> From<L> for SignupList
where
    E: Into<SignupListEntry>,
    L: IntoIterator<Item = E>,
{
    fn from(vals: L) -> Self {
        Self(vals.into_iter().map(Into::into).collect())
    }
}

impl Actor for ListKeeper {
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
