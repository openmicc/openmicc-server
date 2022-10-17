use std::fmt::Debug;

use actix::{Actor, Context, Handler, Message};
use mediasoup::rtp_parameters::RtpCapabilitiesFinalized;
use tracing::{info, instrument};

use crate::{
    signup_list::ListKeeper,
    stage::Stage,
    user_session::{UserSession, WelcomeMessage},
    utils::{MyAddr, SendAndCheckResponse, WrapAddr},
};

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum GreeterMessage {
    /// The user sends their address to the greeter upon arrival
    Hello(MyAddr<UserSession>),
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

/// Info that the Greeter has for new users
#[derive(Debug, Clone)]
pub struct GreeterInfo {
    pub router_rtp_capabilities: RtpCapabilitiesFinalized,
}

#[derive(Debug)]
pub struct AddressBook {
    pub signup_list: MyAddr<ListKeeper>,
    pub stage: MyAddr<Stage>,
}

impl Clone for AddressBook {
    fn clone(&self) -> Self {
        Self {
            signup_list: self.signup_list.clone(),
            stage: self.stage.clone(),
        }
    }
}

pub struct Greeter {
    addrs: AddressBook,
    info: GreeterInfo,
}

impl Greeter {
    pub fn new(addrs: AddressBook, info: GreeterInfo) -> Self {
        Self { addrs, info }
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
                let welcome_msg = WelcomeMessage {
                    addrs,
                    checklist,
                    router_rtp_capabilities: self.info.router_rtp_capabilities.clone(),
                };
                self.send_and_check_response(ctx, user, welcome_msg);
            }
        }
    }
}

#[instrument]
pub fn start_greeter(addrs: AddressBook, info: GreeterInfo) -> MyAddr<Greeter> {
    Greeter::new(addrs, info).start().wrap()
}
