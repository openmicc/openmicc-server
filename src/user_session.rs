use actix::prelude::*;

use actix::{Actor, StreamHandler};
use actix_web_actors::ws;
use anyhow::Context as AnyhowContext;
use serde::{Deserialize, Serialize};

use crate::signup_list::{Signup, SignupList, SignupListActor, SubscribeToSignupList};
use crate::utils::send_or_log_err;

/// Sent from client to sever
#[derive(Debug, Deserialize, Message)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum ClientMessage {
    /// Sign me up.
    SignMeUp(Signup),
    // TODO: ImReady (I'm ready to perform)
}

/// Sent from server to client
#[derive(Serialize, Message)]
#[serde(tag = "action")]
#[serde(rename = "camelCase")]
#[rtype(result = "()")]
enum ServerMessage {
    // TODO: Pop from list (after finishing)
    // TODO: Remove person from list (dropped out early)
    /// The whole current sign-up list.
    SignupList(SignupList),
    /// A notification of a new sign-up.
    NewSignup(Signup),
    // TODO: AreYouReady (ready to perform?)
}

/// A message from the SignupListActor to the UserSession
/// with information about the signup list
#[derive(Clone, Debug, Message, Serialize)]
#[rtype(result = "anyhow::Result<()>")]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum SignupListMessage {
    // TODO Probably better to have a single ServerMessage type.
    All { list: SignupList },
    New { new: Signup },
}

pub struct UserSession {
    signup_list_addr: Addr<SignupListActor>,
}

impl UserSession {
    pub fn new(signup_list_addr: Addr<SignupListActor>) -> Self {
        Self { signup_list_addr }
    }
}

impl Actor for UserSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("started");

        let addr = ctx.address();

        // Subscribe to redis updates
        let subscribe_msg = SubscribeToSignupList(addr);
        send_or_log_err(&self.signup_list_addr, subscribe_msg);

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
        // forward the message over WebSockets to the client, encoded as JSON
        let serialized = serde_json::to_string(&msg).context("serializing SignupListMessage")?;
        println!("Serialized...");
        ctx.text(serialized);

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
            ClientMessage::SignMeUp(signup) => {
                // TODO send message to signup list
                todo!()
            }
        }
    }
}
