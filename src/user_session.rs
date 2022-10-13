use actix::prelude::*;

use actix::{Actor, StreamHandler};
use actix_web_actors::ws;
use anyhow::{bail, Context as AnyhowContext};
use serde::{Deserialize, Serialize};

use crate::greeter::{AddressBook, Greeter, GreeterMessage, OnboardingChecklist, OnboardingTask};
use crate::signup_list::{Signup, SignupList, SubscribeToSignupList};
use crate::utils::send_or_log_err;

/// Sent from client to sever
#[derive(Debug, Message, Deserialize, Serialize)]
#[serde(tag = "action")]
#[serde(rename_all = "camelCase")]
#[rtype(result = "()")]
pub enum ClientMessage {
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
#[derive(Message)]
#[rtype(result = "()")]
pub struct WelcomeMessage {
    pub addrs: AddressBook,
    pub checklist: OnboardingChecklist,
}

pub struct UserSession {
    greeter_addr: Addr<Greeter>,
    state: State,
}

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
    pub fn new(greeter_addr: Addr<Greeter>) -> Self {
        Self {
            greeter_addr,
            state: Default::default(),
        }
    }

    fn subscribe_to_signup_list(&self, ctx: &<Self as Actor>::Context) -> anyhow::Result<()> {
        if let State::Onboarding { addrs } = &self.state {
            let my_addr = ctx.address();
            let subscribe_msg = SubscribeToSignupList(my_addr);
            send_or_log_err(&addrs.signup_list, subscribe_msg);
        } else {
            bail!("can only subscribe to signup list during onboarding");
        }

        Ok(())
    }

    fn do_onboarding_task(
        &self,
        ctx: &<Self as Actor>::Context,
        task: OnboardingTask,
    ) -> anyhow::Result<()> {
        match task {
            OnboardingTask::SubscribeToSignupList => self.subscribe_to_signup_list(ctx),
        }
    }

    fn onboard(
        &mut self,
        ctx: &<Self as Actor>::Context,
        checklist: OnboardingChecklist,
    ) -> anyhow::Result<()> {
        match &self.state {
            State::Onboarding { addrs } => {
                for task in checklist {
                    self.do_onboarding_task(ctx, task)
                        .context("onboarding task")
                        .map_err(|err| eprintln!("{:#}", err))
                        .ok();
                }

                self.state = State::Onboarded {
                    addrs: addrs.clone(),
                };

                println!("User finished onboarding");
            }
            _ => bail!("Can only onboard in Onboarding state"),
        }

        Ok(())
    }

    /// Send a ServerMessage to the client
    fn send_msg(
        &self,
        ctx: &mut <Self as Actor>::Context,
        msg: ServerMessage,
    ) -> anyhow::Result<()> {
        let serialized = serde_json::to_string(&msg).context("serializing ServerMessage")?;
        println!("sending message '{}'", serialized);
        ctx.text(serialized);

        Ok(())
    }
}

impl Actor for UserSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("started");

        // Say hello to the greeter
        let addr = ctx.address();
        let hello_msg = GreeterMessage::Hello(addr);
        send_or_log_err(&self.greeter_addr, hello_msg);

        println!("started done");
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> actix::Running {
        println!("stopping");
        actix::Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("stopped");
        // self.signup_feed.unregister(ctx.address());
    }
}

impl Handler<SignupListMessage> for UserSession {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: SignupListMessage, ctx: &mut Self::Context) -> Self::Result {
        println!("User got signup list message: {:?}", &msg);

        if let State::Fresh = self.state {
            bail!("wasn't expecting SignupListMessage before welcoming");
        }

        match msg {
            SignupListMessage::All { list } => {
                // WelcomeInfo
                let welcome_info = WelcomeInfo { signup_list: list };
                // TODO: This is a bit confusing that we're sending a
                // `ServerMessage::Welcome` in the `Welcomed` state.
                let server_msg = ServerMessage::Welcome(welcome_info);
                self.send_msg(ctx, server_msg)?;
            }
            SignupListMessage::New { new } => {
                // Signup update
                let server_msg = ServerMessage::NewSignup(new);
                self.send_msg(ctx, server_msg)?;
            }
        }

        // forward the message over WebSockets to the client, encoded as JSON
        println!("Serialized...");

        println!("forwarded signup list message over WS");

        Ok(())
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for UserSession {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(message) => {
                    // Parse JSON into an enum and just send it back to the actor to be processed
                    // by another handler below, it is much more convenient to just parse it in one
                    // place and have typed data structure everywhere else
                    send_or_log_err(&ctx.address(), message)
                }
                Err(error) => {
                    eprintln!("Failed to parse client message: {}\n{}", error, text);
                }
            },
            Ok(ws::Message::Binary(bin)) => {
                eprintln!("Unexpected binary message: {:?}", bin);
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

    fn handle(&mut self, msg: ClientMessage, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ClientMessage::SignMeUp { name } => {
                // TODO send message to signup list
                todo!()
            }
        }
    }
}

impl Handler<WelcomeMessage> for UserSession {
    type Result = ();

    fn handle(&mut self, msg: WelcomeMessage, ctx: &mut Self::Context) -> Self::Result {
        match self.state {
            State::Fresh => {
                self.state = State::Onboarding { addrs: msg.addrs };
                self.onboard(ctx, msg.checklist)
                    .context("onboarding - handling welcome message")
                    .map_err(|err| eprintln!("ERR: {:#}", err))
                    .ok();
            }
            _ => eprintln!("WARN: Already welcomed..."),
        }
    }
}
