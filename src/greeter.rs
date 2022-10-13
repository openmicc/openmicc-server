use std::fmt::Debug;

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, WrapFuture};
use anyhow::Context as AnyhowContext;
use futures::FutureExt;
use tracing::{info, instrument};

use crate::{
    signup_list::SignupListActor,
    user_session::{UserSession, WelcomeMessage},
    utils::{LogError, MyAddr},
};

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum GreeterMessage {
    /// The user sends their address to the greeter upon arrival
    Hello(Addr<UserSession>),
}

#[derive(Clone, Debug)]
pub enum OnboardingTask {
    SubscribeToSignupList,
}

#[derive(Clone, Debug)]
pub struct OnboardingChecklist(Vec<OnboardingTask>);

impl OnboardingChecklist {
    pub fn new() -> Self {
        let tasks = vec![OnboardingTask::SubscribeToSignupList];

        Self(tasks)
    }
}

impl IntoIterator for OnboardingChecklist {
    type Item = OnboardingTask;

    type IntoIter = std::vec::IntoIter<OnboardingTask>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug)]
pub struct AddressBook {
    pub signup_list: MyAddr<SignupListActor>,
}

impl Clone for AddressBook {
    fn clone(&self) -> Self {
        Self {
            signup_list: self.signup_list.clone(),
        }
    }
}

pub struct Greeter {
    addrs: AddressBook,
}

impl Greeter {
    pub fn new(addrs: AddressBook) -> Self {
        Self { addrs }
    }
}

impl Actor for Greeter {
    type Context = Context<Self>;

    #[instrument(skip_all)]
    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Started greeter");
    }

    #[instrument(skip_all)]
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Stopped greeter");
    }
}

impl Handler<GreeterMessage> for Greeter {
    type Result = ();

    #[instrument(skip(self, ctx), name = "GreeterMessageHandler")]
    fn handle(&mut self, msg: GreeterMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            GreeterMessage::Hello(user) => {
                let addrs = self.addrs.clone();

                let checklist = OnboardingChecklist::new();

                let welcome_info = WelcomeMessage { addrs, checklist };
                let send_fut = user.send(welcome_info).map(|res| {
                    res.context("sending welcome info to user").log_err();
                });

                let actor_fut = send_fut.into_actor(self);
                ctx.spawn(actor_fut);
            }
        }
    }
}

#[instrument]
pub fn start_greeter(addrs: AddressBook) -> Addr<Greeter> {
    let greeter = Greeter::new(addrs);
    greeter.start()
}
