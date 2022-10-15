use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Deref;

use actix::dev::MessageResponse;
use actix::dev::ToEnvelope;
use actix::prelude::*;
use actix::Actor;
use anyhow::Context as AnyhowContext;
use futures::FutureExt;
use tracing::info;
use tracing::info_span;
use tracing::{error, warn};
use tracing_actix::ActorInstrument;

// /// Things that can be safely ignored
// pub trait Ignorable {}

// /// The unit type is ignorable
// impl Ignorable for () {}

/// Send a message, ensuring it was handled (infallible).
/// Only valid for messages with ignorable responses (returns `()`)
pub trait SendAndCheckResponse: Actor
where
    Self::Context: AsyncContext<Self>,
{
    fn send_and_check_response<M, Dst>(
        &mut self,
        ctx: &mut Self::Context,
        addr: MyAddr<Dst>,
        msg: M,
    ) where
        Dst::Context: ToEnvelope<Dst, M>,
        M: std::fmt::Debug + Message<Result = ()> + Send + 'static,
        M::Result: std::fmt::Debug + Send,
        Dst: Handler<M>;
    // TODO: Are any of these trait bounds redundant? ^
}

impl<A: Actor> SendAndCheckResponse for A
where
    A::Context: AsyncContext<A>,
{
    fn send_and_check_response<M, Dst>(
        &mut self,
        ctx: &mut Self::Context,
        addr: MyAddr<Dst>,
        msg: M,
    ) where
        Dst::Context: ToEnvelope<Dst, M>,
        M: std::fmt::Debug + Message<Result = ()> + Send + 'static,
        M::Result: std::fmt::Debug + Send,
        Dst: Handler<M>,
    {
        let do_later = async move {
            addr.send(msg)
                .await
                .with_context(|| format!("mailbox error for {:?}", addr.clone()))?;

            info!("infallible message delivered successfully to {:?}", &addr);

            Ok(())
        };

        let span = info_span!("check response (infallible) follow-up");
        let actor_fut = do_later
            .map(|res| res.log_err())
            .into_actor(self)
            .actor_instrument(span);
        ctx.spawn(actor_fut);
    }
}

/// Send a message, wait for the response,
/// and log any errors that arrise
pub trait SendAndCheckResult: Actor
where
    Self::Context: AsyncContext<Self>,
{
    // Send a message and ensure that it was delivered
    fn send_and_check_result<M: Message, E, Dst>(
        &mut self,
        ctx: &mut Self::Context,
        addr: MyAddr<Dst>,
        msg: M,
    ) where
        E: Send + Sync + 'static,
        Dst::Context: ToEnvelope<Dst, M>,
        M: std::fmt::Debug + Message<Result = Result<(), E>> + Send + 'static,
        M::Result: std::fmt::Debug + Send,
        Result<(), E>: MessageResponse<Dst, M> + AnyhowContext<(), E>,
        Dst: Handler<M>;
    // TODO: Are any of these trait bounds redundant? ^
}

impl<A: Actor> SendAndCheckResult for A
where
    A::Context: AsyncContext<A>,
{
    fn send_and_check_result<M: Message, E, Dst>(
        &mut self,
        ctx: &mut Self::Context,
        addr: MyAddr<Dst>,
        msg: M,
    ) where
        E: Send + Sync + 'static,
        Dst::Context: ToEnvelope<Dst, M>,
        M: std::fmt::Debug + Message<Result = Result<(), E>> + Send + 'static,
        M::Result: std::fmt::Debug + Send,
        Result<(), E>: MessageResponse<Dst, M> + AnyhowContext<(), E>,
        Dst: Handler<M>,
    {
        let do_later = async move {
            addr.send(msg)
                .await
                .with_context(|| format!("mailbox error for {:?}", addr.clone()))?
                .context("handler returned an error")?;

            info!(
                "message delivered successfully to {:?}, and result is good.",
                &addr
            );

            Ok(())
        };

        let span = info_span!("check result (fallible) follow-up");
        let actor_fut = do_later
            .map(|res| res.log_err())
            .into_actor(self)
            .actor_instrument(span);
        ctx.spawn(actor_fut);
    }
}

/// Produce an event in case of error
pub trait LogError {
    fn log_err(self);

    fn log_warn(self);
}

impl LogError for anyhow::Error {
    fn log_err(self) {
        error!("{:?}", self);
    }

    fn log_warn(self) {
        warn!("{:?}", self);
    }
}

impl LogError for anyhow::Result<()> {
    fn log_err(self) {
        if let Err(err) = self {
            err.log_err()
        }
    }

    fn log_warn(self) {
        if let Err(err) = self {
            err.log_warn()
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
        self.map_err(LogError::log_err).ok()
    }

    fn ok_log_warn(self) -> Option<Self::Target> {
        self.map_err(LogError::log_warn).ok()
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
