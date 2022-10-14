use actix::prelude::*;
use actix::{Actor, StreamHandler};
use actix_web_actors::ws;
use anyhow::{bail, Context as AnyhowContext};
use serde::{Deserialize, Serialize};
use tracing::{error, info, info_span, instrument, warn};
use tracing_actix::ActorInstrument;

use crate::greeter::{AddressBook, Greeter, GreeterMessage, OnboardingChecklist, OnboardingTask};
use crate::signup_list::user_api::GetList;
use crate::signup_list::{Signup, SignupList, SignupListActor, SubscribeToSignupList};
use crate::utils::{LogError, MyAddr, SendAndCheckResponse, SendAndCheckResult, WrapAddr};

/// Sent from client to sever
#[derive(Debug, Message, Deserialize, Serialize)]
#[serde(tag = "action")]
#[serde(rename_all = "camelCase")]
#[rtype(result = "()")]
pub enum ClientMessage {
    /// Get the whole current signup list
    GetList,
    /// Sign me up.
    SignMeUp { name: Signup },
    // TODO: ImReady (I'm ready to perform)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomeInfo {
    /// Current signup list
    signup_list: SignupList,
}

/// Info sent to the client upon connecting to the server
/// Sent from server to client
#[derive(Debug, Message, Deserialize, Serialize)]
#[serde(tag = "action")]
#[serde(rename = "camelCase")]
#[rtype(result = "()")]
pub enum ServerMessage {
    Welcome(WelcomeInfo),
    // TODO: Pop from list (after finishing)
    // TODO: Remove person from list (dropped out early)
    // The whole current sign-up list.
    // SignupList(SignupList),
    /// A notification of a new sign-up.
    NewSignup(Signup),
    /// A snapshot of the whole current sign-up list.
    WholeSignupList {
        list: SignupList,
    },
    // TODO: AreYouReady (ready to perform?)
}

/// A message from the SignupListActor to the UserSession
/// with information about the signup list
#[derive(Clone, Debug, Message)]
#[rtype(result = "anyhow::Result<()>")]
pub enum SignupListMessage {
    All { list: SignupList },
    New { new: Signup },
}

/// Sent from `Greeter` to `UserSession` upon connection
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct WelcomeMessage {
    pub addrs: AddressBook,
    pub checklist: OnboardingChecklist,
}

#[derive(Debug)]
pub struct UserSession {
    greeter_addr: MyAddr<Greeter>,
    state: State,
}

#[derive(Debug, Clone)]
enum State {
    /// Just connected, haven't done anything yet
    Fresh,
    /// We've already received the `WelcomeMessage`, but haven't finished onboarding
    Onboarding {
        /// Address book from the Greeter
        addrs: AddressBook,
    },
    /// Finished onboarding
    Onboarded {
        /// Address book from the Greeter
        addrs: AddressBook,
    },
}

impl Default for State {
    fn default() -> Self {
        Self::Fresh
    }
}

impl UserSession {
    #[instrument]
    pub fn new(greeter_addr: MyAddr<Greeter>) -> Self {
        Self {
            greeter_addr,
            state: Default::default(),
        }
    }

    #[instrument(skip(ctx))]
    fn get_signup_list_inner(
        &self,
        ctx: &mut <Self as Actor>::Context,
        dest: MyAddr<SignupListActor>,
    ) -> anyhow::Result<()> {
        let list_res_fut = async move {
            let current_list = dest
                .send(GetList)
                .await
                .context("mailbox error")?
                .context("getting signup list")?;

            info!("Current signup list: {:?}", current_list);

            Ok(current_list)
        };

        let span = info_span!("UserSession handling reply from SignupListActor");
        let actor_fut = list_res_fut.into_actor(self).actor_instrument(span);

        let do_later = actor_fut
            .map(|list_res: anyhow::Result<SignupList>, act, ctx| {
                info!("Now is later. Result = {:?}", list_res);
                let msg = ServerMessage::WholeSignupList { list: list_res? };
                act.send_msg(ctx, msg)
                    .context("sending message to client")?;
                info!("message has been sent.");

                Ok(())
            })
            .map(|res, _, _| res.log_err());

        ctx.spawn(do_later);

        Ok(())
    }

    #[instrument(skip(ctx))]
    fn get_signup_list(&self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        match &self.state {
            State::Fresh => bail!("cannot get signup list before onboarding"),
            // TODO: refactor state
            State::Onboarding { addrs } => {
                let dest = addrs.signup_list.clone();
                self.get_signup_list_inner(ctx, dest)?;
            }
            State::Onboarded { addrs } => {
                let dest = addrs.signup_list.clone();
                self.get_signup_list_inner(ctx, dest)?;
            }
        }

        Ok(())
    }

    #[instrument(skip(ctx))]
    fn subscribe_to_signup_list(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
    ) -> anyhow::Result<()> {
        if let State::Onboarding { addrs } = &self.state {
            let my_addr = ctx.address();
            let subscribe_msg = SubscribeToSignupList(my_addr.wrap());
            let signup_list = addrs.signup_list.clone();
            self.send_and_check_result(ctx, signup_list, subscribe_msg)
        } else {
            bail!("can only subscribe to signup list during onboarding");
        }

        Ok(())
    }

    #[instrument(skip(ctx))]
    fn unsubscribe_from_signup_list(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
    ) -> anyhow::Result<()> {
        if let State::Onboarding { addrs } = &self.state {
            // TODO: UNSUBSCRIBE
            let my_addr = ctx.address();
            let subscribe_msg = SubscribeToSignupList(my_addr.wrap());
            let signup_list = addrs.signup_list.clone();
            self.send_and_check_result(ctx, signup_list, subscribe_msg)
        } else {
            bail!("can only subscribe to signup list during onboarding");
        }

        Ok(())
    }

    #[instrument(skip(ctx))]
    fn send_unsubscribe_request(
        &self,
        ctx: &<Self as Actor>::Context,
        signup_list: MyAddr<SignupListActor>,
    ) -> anyhow::Result<()> {
        // TODO
        todo!()
        // let x = 2;
        // let msg = SignupListMessage::
        // signup_list.send
    }

    #[instrument(skip(ctx))]
    fn do_onboarding_task(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        task: OnboardingTask,
    ) -> anyhow::Result<()> {
        match task {
            OnboardingTask::SubscribeToSignupList => self.subscribe_to_signup_list(ctx),
        }
    }

    #[instrument(skip(ctx))]
    fn onboard(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        checklist: OnboardingChecklist,
    ) -> anyhow::Result<()> {
        match &self.state {
            State::Onboarding { addrs } => {
                let cloned_addrs = addrs.clone();
                for task in checklist {
                    self.do_onboarding_task(ctx, task)
                        .context("onboarding task")
                        .log_err();
                }

                self.state = State::Onboarded {
                    addrs: cloned_addrs,
                };

                info!("User finished onboarding");
            }
            _ => bail!("Can only onboard in Onboarding state"),
        }

        Ok(())
    }

    /// Send a ServerMessage to the client
    #[instrument(skip(ctx))]
    fn send_msg(
        &self,
        ctx: &mut <Self as Actor>::Context,
        msg: ServerMessage,
    ) -> anyhow::Result<()> {
        let serialized = serde_json::to_string(&msg).context("serializing ServerMessage")?;
        info!("sending message '{}'", serialized);
        ctx.text(serialized);
        info!("message sent.");

        Ok(())
    }
}

impl Actor for UserSession {
    type Context = ws::WebsocketContext<Self>;

    #[instrument(skip(ctx))]
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("started");

        // Say hello to the greeter
        let addr = ctx.address().wrap();
        let hello_msg = GreeterMessage::Hello(addr);
        let greeter = self.greeter_addr.clone();
        self.send_and_check_response(ctx, greeter, hello_msg);

        info!("started done");
    }

    #[instrument(skip(_ctx))]
    fn stopping(&mut self, _ctx: &mut Self::Context) -> actix::Running {
        info!("stopping");
        actix::Running::Stop
    }

    #[instrument(skip(ctx))]
    fn stopped(&mut self, ctx: &mut Self::Context) {
        info!("stopped");
        // TODO
        todo!()
        // self.signup_feed.unregister(ctx.address());
    }
}

impl Handler<SignupListMessage> for UserSession {
    type Result = anyhow::Result<()>;

    #[instrument(skip(ctx), name = "SignupListMessageHandler")]
    fn handle(&mut self, msg: SignupListMessage, ctx: &mut Self::Context) -> Self::Result {
        info!("User got signup list message: {:?}", &msg);

        if let State::Fresh = self.state {
            bail!("wasn't expecting SignupListMessage before welcoming");
        }

        info!("made it this far");

        match msg {
            SignupListMessage::All { list } => {
                // WelcomeInfo
                let welcome_info = WelcomeInfo { signup_list: list };
                // TODO: This is a bit confusing that we're sending a
                // `ServerMessage::Welcome` in the `Welcomed` state.
                let server_msg = ServerMessage::Welcome(welcome_info);
                self.send_msg(ctx, server_msg).context("got whole list")?;
            }
            SignupListMessage::New { new } => {
                // Signup update
                let server_msg = ServerMessage::NewSignup(new);
                self.send_msg(ctx, server_msg).context("got list update")?;
                warn!("Update has been forwarded to client");
            }
        }

        // forward the message over WebSockets to the client, encoded as JSON
        info!("Serialized...");

        info!("forwarded signup list message over WS");

        Ok(())
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for UserSession {
    #[instrument(skip(ctx), name = "WsMessageStreamHandler")]
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(msg) => {
                    // Parse JSON into an enum and just send it back to the actor to be processed
                    // by another handler below, it is much more convenient to just parse it in one
                    // place and have typed data structure everywhere else
                    let addr = ctx.address().clone().wrap();
                    self.send_and_check_response(ctx, addr, msg);
                }
                Err(error) => {
                    error!("Failed to parse client message: {}\n{}", error, text);
                }
            },
            Ok(ws::Message::Binary(bin)) => {
                error!("Unexpected binary message: {:?}", bin);
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

impl Handler<ClientMessage> for UserSession {
    type Result = ();

    #[instrument(skip(ctx), name = "ClientMessageHandler")]
    fn handle(&mut self, msg: ClientMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ClientMessage::SignMeUp { name } => {
                // TODO send message to signup list
                todo!()
            }
            ClientMessage::GetList => {
                self.get_signup_list(ctx)
                    .context("getting signup list")
                    .log_err();
            }
        }
    }
}

impl Handler<WelcomeMessage> for UserSession {
    type Result = ();

    #[instrument(skip_all, name = "WelcomeMessageHandler")]
    fn handle(&mut self, msg: WelcomeMessage, ctx: &mut Self::Context) -> Self::Result {
        match self.state {
            State::Fresh => {
                self.state = State::Onboarding { addrs: msg.addrs };
                self.onboard(ctx, msg.checklist)
                    .context("onboarding")
                    .log_err();
            }
            _ => warn!("Already welcomed..."),
        }
    }
}
