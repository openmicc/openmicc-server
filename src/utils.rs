use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Deref;

use actix::dev::ToEnvelope;
use actix::prelude::*;
use actix::Actor;
use anyhow::Context as AnyhowContext;
use tracing::info;
use tracing::{error, warn};
use uuid::Uuid;

pub async fn send_or_log_err_inner<A, M>(addr: Addr<A>, msg: M)
where
    A: Actor,
    A::Context: ToEnvelope<A, M>,
    M: std::fmt::Debug + Message + Send + 'static,
    M::Result: std::fmt::Debug + Send,
    A: Handler<M>,
{
    let msg_uuid = Uuid::new_v4();
    info!(?msg, ?msg_uuid, "sending mesage");
    addr.send(msg)
        .await
        .context("delivering message")
        .map(|resp| info!(?resp, "message response"))
        .log_err();
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

/// Produce an event in case of error
pub trait LogError {
    fn log_err(self);

    fn log_warn(self);
}

fn log_anyhow_err(err: anyhow::Error) {
    error!("{:?}", err);
}

fn log_anyhow_warn(err: anyhow::Error) {
    warn!("{:?}", err);
}

impl LogError for anyhow::Result<()> {
    fn log_err(self) {
        if let Err(err) = self {
            log_anyhow_err(err)
        }
    }

    fn log_warn(self) {
        if let Err(err) = self {
            log_anyhow_warn(err)
        }
    }
}

/// Turn an error into an option after logging
pub trait LogOk {
    type Target;
    fn ok_log_err(self) -> Option<Self::Target>;

    fn ok_log_warn(self) -> Option<Self::Target>;
}

impl<T> LogOk for anyhow::Result<T> {
    type Target = T;

    fn ok_log_err(self) -> Option<Self::Target> {
        self.map_err(log_anyhow_err).ok()
    }

    fn ok_log_warn(self) -> Option<Self::Target> {
        self.map_err(log_anyhow_warn).ok()
    }
}

/// Ignore an error after doing just one more thing.
pub trait OkJust {
    type Error;
    fn ok_just<F: Fn(Self::Error)>(self, f: F);
}

impl OkJust for anyhow::Result<()> {
    type Error = anyhow::Error;

    fn ok_just<F: Fn(Self::Error)>(self, f: F) {
        if let Err(err) = self {
            f(err);
        }
    }
}

pub trait DisplayHash: Hash {
    fn display(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        let bytes = hasher.finish();
        format!("{:x}", bytes)
    }
}

impl<A: Actor> DisplayHash for MyAddr<A> {}

pub struct MyAddr<A: Actor>(Addr<A>);

impl<A: Actor> PartialEq for MyAddr<A> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<A: Actor> Eq for MyAddr<A> {
    fn assert_receiver_is_total_eq(&self) {}
}

impl<A: Actor> Clone for MyAddr<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A: Actor> Hash for MyAddr<A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<A: Actor> Deref for MyAddr<A> {
    type Target = Addr<A>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A: Actor> Debug for MyAddr<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Addr").field(&self.display()).finish()
    }
}

pub trait WrapAddr {
    type Target: Actor;

    fn wrap(self) -> MyAddr<Self::Target>;
}

impl<A: Actor> WrapAddr for Addr<A> {
    type Target = A;

    fn wrap(self) -> MyAddr<Self::Target> {
        MyAddr(self)
    }
}
