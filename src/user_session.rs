use std::ops::Deref;

use actix::prelude::*;
use actix::{Actor, StreamHandler};
use actix_web_actors::ws;
use anyhow::{anyhow, bail, Context as AnyhowContext};
use mediasoup::rtp_parameters::RtpParameters;
use serde::{Deserialize, Serialize};
use tracing::{error, info, info_span, instrument, warn};
use tracing_actix::ActorInstrument;

use crate::greeter::{AddressBook, Greeter, GreeterMessage, OnboardingChecklist, OnboardingTask};
use crate::signup_list::user_api::{GetList, SignMeUp, Subscribe, TakeMeOff, Unsubscribe};
use crate::signup_list::{ListKeeper, SignupList};
use crate::signup_list_entry::{IdAndReceipt, SignupId, SignupListEntry, SignupListEntryText};
use crate::signup_receipt::SignupReceipt;
use crate::stage::messages::incoming::Perform;
use crate::stage::messages::outgoing::{ViewParams, YourTurn};
use crate::stage::TransportOptions;
use crate::utils::{LogError, LogOk, MyAddr, SendAndCheckResponse, SendAndCheckResult, WrapAddr};

type SignupListCounterInner = usize;

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct SignupListCounter(SignupListCounterInner);

impl Deref for SignupListCounter {
    type Target = SignupListCounterInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SignupListCounter {
    /// Increment the counter and return a reference
    /// to the updated value.
    pub fn incr(&mut self) -> &Self {
        self.0 += 1;
        &*self
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformParams {
    pub signup_receipt: SignupReceipt,
    // TODO: Should the rtp params be sent during init?
    pub rtp_parameters: RtpParameters,
}

/// Sent from client to sever
#[derive(Debug, Message, Deserialize, Serialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "camelCase")]
#[rtype(result = "()")]
pub enum ClientMessage {
    /// Get the whole current signup list
    GetList,
    /// Sign me up.
    SignMeUp(SignupListEntryText),
    /// Take me off the list.
    TakeMeOff(IdAndReceipt),
    // TODO: ImReady (I'm ready to perform)
    // I'm ready to perform.
    ImReady(PerformParams),
}

/// Info sent to the client upon connecting to the server
/// Sent from server to client
#[derive(Debug, Message, Deserialize, Serialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "camelCase")]
#[rtype(result = "()")]
pub enum ServerMessage {
    // TODO: Pop from list (after finishing)
    // TODO: Remove person from list (dropped out early)
    // The whole current sign-up list.
    // SignupList(SignupList),
    /// A notification of a new sign-up.
    NewSignup {
        /// The new list entry
        entry: SignupListEntry,
        /// Count list updates so that
        /// clients can tell if they've missed one
        /// and ask for the whole list
        counter: SignupListCounter,
    },

    /// An entry has been deleted from the sign-up list.
    ListRemoval {
        /// The id of the removed entry
        id: SignupId,
        /// Count list updates so that
        /// clients can tell if they've missed one
        /// and ask for the whole list
        counter: SignupListCounter,
    },

    /// A snapshot of the whole current sign-up list.
    WholeSignupList(SignupList),

    /// The user has successfully signed up and obtained a receipt.
    SignupSuccess {
        id: SignupId,
        receipt: SignupReceipt,
    }, // TODO: AreYouReady (ready to perform?)

    /// The user can now start watching.
    StartWatching {
        consumer_transport_options: TransportOptions,
    },

    /// The user can now start performing.
    StartPerforming {
        producer_transport_options: TransportOptions,
    },
}

/// A message from the SignupListActor to the UserSession
/// with information about the signup list
#[derive(Clone, Debug, Message)]
#[rtype(result = "anyhow::Result<()>")]
pub enum SignupListMessage {
    All {
        list: SignupList,
        // TODO: Include counter here as well?
        // (probably should refactor into
        // CountedSignupListMessage { msg: SignupListMessage, counter: SignupListCounter } ot something)
        // and then enum SignupListMessage { All(SignupList), Add(SignupListEntry), Del(SignupId) }
    },
    Add {
        new: SignupListEntry,
        counter: SignupListCounter,
    },
    Del {
        id: SignupId,
        counter: SignupListCounter,
    },
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
    /// Whether the user has completed onboarding
    onboarded: bool,
    /// Address book from the Greeter
    addrs: Option<AddressBook>,
}

impl UserSession {
    #[instrument]
    pub fn new(greeter_addr: MyAddr<Greeter>) -> Self {
        Self {
            greeter_addr,
            onboarded: Default::default(),
            addrs: Default::default(),
        }
    }

    #[instrument(skip(ctx))]
    fn get_signup_list_inner(
        &self,
        ctx: &mut <Self as Actor>::Context,
        dest: MyAddr<ListKeeper>,
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
                let msg = ServerMessage::WholeSignupList(list_res?);
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
    fn sign_me_up(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        text: SignupListEntryText,
    ) -> anyhow::Result<()> {
        let addrs = self.addrs.as_ref().ok_or(anyhow!("no address book"))?;

        let signup_msg = SignMeUp(text);
        let signup_list = addrs.signup_list.clone();

        let receipt_res_fut = async move {
            let receipt = signup_list
                .send(signup_msg)
                .await
                .context("signup list mailbox error")?
                .context("signup failure")?;

            Ok::<_, anyhow::Error>(receipt)
        };

        let span = info_span!("forwarding signup receipt to client");
        let do_later =
            receipt_res_fut
                .into_actor(self)
                .actor_instrument(span)
                .map(|res, act, ctx| {
                    let IdAndReceipt { id, receipt } = res?;
                    let msg = ServerMessage::SignupSuccess { id, receipt };
                    act.send_msg(ctx, msg)?;
                    Ok(())
                });

        let logged = do_later.map(|res, _, _| {
            res.ok_log_err();
        });

        ctx.wait(logged);

        // self.send_and_check_result(ctx, signup_list, signup_msg);

        Ok(())
    }

    // TODO: Move above sign_me_up
    #[instrument(skip(ctx))]
    fn get_signup_list(&self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        let addrs = self.addrs.as_ref().ok_or(anyhow!("no address book"))?;
        let dest = addrs.signup_list.clone();

        self.get_signup_list_inner(ctx, dest)?;

        Ok(())
    }

    #[instrument(skip(ctx))]
    fn cancel_signup(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        id_and_receipt: IdAndReceipt,
    ) -> anyhow::Result<()> {
        let addrs = self.addrs.as_ref().ok_or(anyhow!("no address book"))?;
        let signup_list = addrs.signup_list.clone();

        let cancel_msg = TakeMeOff(id_and_receipt);
        self.send_and_check_result(ctx, signup_list, cancel_msg);

        Ok(())
    }

    #[instrument(skip(ctx))]
    fn subscribe_to_signup_list(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
    ) -> anyhow::Result<()> {
        let addrs = self.addrs.as_ref().ok_or(anyhow!("no address book"))?;
        let signup_list = addrs.signup_list.clone();
        let my_addr = ctx.address().wrap();

        let subscribe_msg = Subscribe(my_addr);
        self.send_and_check_response(ctx, signup_list, subscribe_msg);

        Ok(())
    }

    #[instrument(skip(ctx))]
    fn unsubscribe_from_signup_list(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
    ) -> anyhow::Result<()> {
        let addrs = self.addrs.as_ref().ok_or(anyhow!("no address book"))?;
        let signup_list = addrs.signup_list.clone();
        let my_addr = ctx.address().wrap();

        let unsubscribe_msg = Unsubscribe(my_addr);
        self.send_and_check_response(ctx, signup_list, unsubscribe_msg);

        Ok(())
    }

    fn start_performing(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        params: PerformParams,
    ) -> anyhow::Result<()> {
        let addrs = self.addrs.as_ref().ok_or(anyhow!("no address book"))?;
        let stage = addrs.stage.clone();

        let rtp_parameters = params.rtp_parameters;
        let addr = ctx.address().wrap();
        let perform_msg = Perform {
            rtp_parameters,
            addr,
        };
        self.send_and_check_response(ctx, stage, perform_msg);

        Ok(())
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
        if self.onboarded {
            bail!("already onboarded");
        }

        for task in checklist {
            self.do_onboarding_task(ctx, task)
                .context("onboarding task")
                .log_err();
        }

        // yay
        self.onboarded = true;

        info!("User finished onboarding");

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
        self.unsubscribe_from_signup_list(ctx)
            .context("unsubscribing from signup list")
            .log_err();

        info!("stopped");
    }
}

impl Handler<SignupListMessage> for UserSession {
    type Result = anyhow::Result<()>;

    #[instrument(skip(ctx), name = "SignupListMessageHandler")]
    fn handle(&mut self, msg: SignupListMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            SignupListMessage::All { list } => {
                // WelcomeInfo
                let server_msg = ServerMessage::WholeSignupList(list);
                self.send_msg(ctx, server_msg).context("got whole list")?;
            }
            SignupListMessage::Add { new, counter } => {
                // Signup update
                let server_msg = ServerMessage::NewSignup {
                    entry: new,
                    counter,
                };
                self.send_msg(ctx, server_msg)
                    .context("got list addition")?;
            }
            SignupListMessage::Del { id, counter } => {
                let server_msg = ServerMessage::ListRemoval { id, counter };
                self.send_msg(ctx, server_msg).context("got list removal")?;
            }
        }

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
            ClientMessage::SignMeUp(text) => {
                self.sign_me_up(ctx, text)
                    .context("handling ClientMessage::SignMeUp")
                    .log_err();
            }
            ClientMessage::GetList => {
                self.get_signup_list(ctx)
                    .context("handling ClientMessage::GetList")
                    .log_err();
            }
            ClientMessage::TakeMeOff(id_and_receipt) => {
                self.cancel_signup(ctx, id_and_receipt)
                    .context("handling ClientMessage::TakeMeOff")
                    .log_err();
            }
            ClientMessage::ImReady(perform_params) => {
                self.start_performing(ctx, perform_params)
                    .context("handling ClientMessage::ImReady")
                    .log_err();
            }
        }
    }
}

impl Handler<WelcomeMessage> for UserSession {
    type Result = ();

    #[instrument(skip_all, name = "WelcomeMessageHandler")]
    fn handle(&mut self, msg: WelcomeMessage, ctx: &mut Self::Context) -> Self::Result {
        if self.onboarded {
            warn!("Already welcomed...");
        } else {
            self.addrs.replace(msg.addrs);
            self.onboard(ctx, msg.checklist)
                .context("onboarding")
                .log_err();

            // TODO: Should this be initiated by the client?
            // Get signup list right after onboarding
            self.get_signup_list(ctx).log_err();
        }
    }
}

// Handlers for messages from stage

impl Handler<ViewParams> for UserSession {
    type Result = ();

    fn handle(&mut self, msg: ViewParams, ctx: &mut Self::Context) -> Self::Result {
        let server_msg = ServerMessage::StartWatching {
            consumer_transport_options: msg.consumer_transport_options,
        };
        self.send_msg(ctx, server_msg).log_err();
    }
}

impl Handler<YourTurn> for UserSession {
    type Result = ();

    fn handle(&mut self, msg: YourTurn, ctx: &mut Self::Context) -> Self::Result {
        let server_msg = ServerMessage::StartPerforming {
            producer_transport_options: msg.producer_transport_options,
        };
        self.send_msg(ctx, server_msg).log_err();
    }
}
