use actix::prelude::*;

use actix::{Actor, StreamHandler};
use actix_web_actors::ws;
use anyhow::{bail, Context as AnyhowContext};
use serde::{Deserialize, Serialize};

use crate::greeter::{Greeter, GreeterMessage};
use crate::signup_list::{Signup, SignupList};
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
    // TODO Probably better to have a single ServerMessage type.
    All { list: SignupList },
    New { new: Signup },
}

pub struct UserSession {
    greeter_addr: Addr<Greeter>,
    welcomed: bool,
}

impl UserSession {
    pub fn new(greeter_addr: Addr<Greeter>) -> Self {
        Self {
            greeter_addr,
            welcomed: false,
        }
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

        match (self.welcomed, msg) {
            (false, SignupListMessage::New { .. }) => {
                bail!("wasn't expecting update before welcome");
            }
            (true, SignupListMessage::All { .. }) => {
                bail!("wasn't expecing whole list after welcome");
            }
            (true, SignupListMessage::New { new }) => {
                // Signup update
                let server_msg = ServerMessage::NewSignup(new);
                self.send_msg(ctx, server_msg)?;
            }
            (false, SignupListMessage::All { list }) => {
                // WelcomeIngo
                let welcome_info = WelcomeInfo { signup_list: list };
                let server_msg = ServerMessage::Welcome(welcome_info);
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
