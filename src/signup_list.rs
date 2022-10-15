use std::collections::HashSet;

use actix::prelude::*;
use actix::{Actor, Context};
use anyhow::{anyhow, bail, Context as AnyhowContext};
use redis::{Client as RedisClient, Commands, Connection as RedisConnection, RedisResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::constants::{REMOVAL_TOPIC, SIGNUP_COUNTER, SIGNUP_LIST, SIGNUP_TOPIC};
use crate::redis_subscriber::{RedisMessage, RedisSubscriber, RedisSubscriberMessage};
use crate::signup_list_entry::{
    EntryAndReceipt, IdAndReceipt, SignupId, SignupListEntry, SignupListEntryText,
};
use crate::signup_receipt::SignupReceipt;
use crate::user_session::{SignupListCounter, SignupListMessage, UserSession};
use crate::utils::{LogError, MyAddr, SendAndCheckResponse, SendAndCheckResult, WrapAddr};

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

                let counter = self.update_counter.incr().clone();

                let msg = match topic.as_str() {
                    SIGNUP_TOPIC => {
                        let decoded: SignupListEntry =
                            serde_json::from_str(&content).context("decoding update content")?;

                        SignupListMessage::Add {
                            new: decoded,
                            counter,
                        }
                    }
                    REMOVAL_TOPIC => {
                        let id = content.parse().context("parsing removal id")?;
                        SignupListMessage::Del { id, counter }
                    }
                    _ => bail!("got weird topic: {}", topic),
                };

                // Dispatch messages
                let users = self.users.clone();
                for user in users {
                    self.send_and_check_result(ctx, user, msg.clone())
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

    /// Take my name off the list
    #[derive(Debug, Message)]
    #[rtype(result = "anyhow::Result<()>")]
    pub struct TakeMeOff(pub IdAndReceipt);

    impl Handler<TakeMeOff> for ListKeeper {
        type Result = anyhow::Result<()>;

        #[instrument(skip(self, _ctx))]
        fn handle(&mut self, msg: TakeMeOff, _ctx: &mut Self::Context) -> Self::Result {
            info!("TakeMeOff handler");
            let id_and_receipt = msg.0;
            // Check that the signup belongs to the user
            let id = self.check_receipt(id_and_receipt)?;
            info!("receipt is good.");
            self.remove_entry(id)?;

            Ok(())
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

/// Struct Lifecycle
impl ListKeeper {
    #[instrument]
    pub fn try_new(client: RedisClient) -> anyhow::Result<Self> {
        let redis = client.get_connection()?;

        let new = Self {
            client,
            redis,
            users: Default::default(),
            update_counter: Default::default(),
        };

        Ok(new)
    }

    #[instrument(skip_all)]
    fn subscribe_to_redis_topic(&mut self, ctx: &mut <Self as Actor>::Context, topic: &str) {
        // Register for updates & start subscriber
        let subscriber = RedisSubscriber::<Self>::new(self.client.clone(), topic);

        let addr = ctx.address().wrap();
        let subscriber_addr = subscriber.start().wrap();

        let register_msg = RedisSubscriberMessage::Register(addr);
        self.send_and_check_response(ctx, subscriber_addr, register_msg);
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
}

/// sub-CRUD(?) Operations
impl ListKeeper {
    #[instrument(skip(self))]
    fn publish_signup(&mut self, entry_and_receipt: EntryAndReceipt) -> anyhow::Result<()> {
        // TODO: Publish asynchronously?
        let json_entry =
            serde_json::to_string(&entry_and_receipt.entry).context("encoding signup entry")?;
        let json_entry_and_receipt = serde_json::to_string(&entry_and_receipt)
            .context("encoding signup entry and receipt")?;

        // TODO: Not sure if this really necessitates a transaction
        let tx_res = redis::transaction(&mut self.redis, &[SIGNUP_LIST], |conn, pipe| {
            pipe.rpush(SIGNUP_LIST, &json_entry_and_receipt)
                .publish(SIGNUP_TOPIC, &json_entry)
                .query(conn)
        });

        // TODO: Is this necessary?
        let _unit: () = tx_res?;

        Ok(())
    }

    /// Check given receipt against receipts stored in redis.
    #[instrument(skip(self))]
    fn check_receipt(&mut self, given: IdAndReceipt) -> anyhow::Result<SignupId> {
        let matched = self
            .get_list_with_receipts()?
            .into_iter()
            .find(|candidate| candidate.entry.id == given.id)
            .ok_or(anyhow!("No list entry with given id {:?}", given.id))?;

        if matched.receipt == given.receipt {
            Ok(matched.entry.id)
        } else {
            bail!(
                "A forged receipt, eh? ({:?} != {:?})",
                matched.receipt,
                given.receipt
            )
        }
    }

    /// No receipts
    #[instrument(skip(self))]
    fn get_list(&mut self) -> anyhow::Result<SignupList> {
        self.get_list_as().map(SignupList)
    }

    /// With receipts
    #[instrument(skip(self))]
    fn get_list_with_receipts(&mut self) -> anyhow::Result<Vec<EntryAndReceipt>> {
        self.get_list_as()
    }

    #[instrument(skip(self))]
    fn get_list_as<T: DeserializeOwned>(&mut self) -> anyhow::Result<Vec<T>> {
        get_list_as(&mut self.redis)
    }

    #[instrument(skip(self))]
    fn remove_entry(&mut self, id: SignupId) -> anyhow::Result<()> {
        // TODO: Could probably be implemented more efficiently

        let tx_res = redis::transaction(&mut self.redis, &[SIGNUP_LIST], |conn, pipe| {
            let mut inner = || -> anyhow::Result<()> {
                let raw_list: Vec<String> = get_raw_list(conn).context("getting raw list")?;
                let deserialized_list: Vec<SignupListEntry> =
                    deserialize_list(&raw_list).context("deserializing raw list")?;

                let index = deserialized_list
                    .into_iter()
                    .position(|candidate| candidate.id == id)
                    .ok_or(anyhow!("No list entry with id {:?} to remove", id))?;

                // Redis insists that we remove list items by value, not index
                let value = raw_list
                    .get(index)
                    .ok_or(anyhow!("index issue in remove_entry?"))?;

                // NOTE: This might behave unexpectedly if there are
                // identical duplicate entries in the list for any reason
                // (there shouldn't be)
                pipe.lrem(SIGNUP_LIST, 1, value);

                // publish update for subscribers after removing
                pipe.publish(REMOVAL_TOPIC, id);

                Ok(())
            };
            //

            if let Err(err) = inner() {
                // NOTE: If there is an error constructing the pipeline,
                // the caller will _NOT_ be notified (but it will be logged)
                err.log_err();
                // Flush (without executing) any queued commands
                pipe.clear();
            }

            pipe.query(conn)
        });

        // TODO: Is this necessary?
        let _unit: () = tx_res?;

        // TODO: Return value should indicate whether
        // the transaction succeeded.
        Ok(())
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

impl SignupList {
    pub fn into_iter(self) -> impl Iterator<Item = SignupListEntry> {
        self.0.into_iter()
    }
}

impl Actor for ListKeeper {
    type Context = Context<Self>;

    #[instrument(skip_all)]
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Start signup list");

        self.subscribe_to_redis_topic(ctx, SIGNUP_TOPIC);
        self.subscribe_to_redis_topic(ctx, REMOVAL_TOPIC);

        info!("signup list actor start finished");
    }

    #[instrument(skip_all)]
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Stop signup list");
    }
}

#[instrument(skip(conn))]
fn get_raw_list(conn: &mut RedisConnection) -> RedisResult<Vec<String>> {
    let result = conn.lrange(SIGNUP_LIST, 0, -1);
    let encoded_list: Vec<String> = result?;

    Ok(encoded_list)
}

fn deserialize_list<T: DeserializeOwned>(raw_list: &Vec<String>) -> serde_json::Result<Vec<T>> {
    raw_list
        .iter()
        .map(|encoded| serde_json::from_str(encoded))
        .collect()
}

#[instrument(skip(conn))]
fn get_list_as<T: DeserializeOwned>(conn: &mut RedisConnection) -> anyhow::Result<Vec<T>> {
    let raw_list = get_raw_list(conn).context("getting raw list")?;
    let list = deserialize_list(&raw_list).context("deserializing list")?;

    Ok(list)
}
