use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, WrapFuture};
use anyhow::Context as AnyhowContext;
use futures::FutureExt;

use crate::{
    signup_list::{SignupListActor, SubscribeToSignupList},
    user_session::UserSession,
};

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum GreeterMessage {
    /// The user sends their address to the greeter upon arrival
    Hello(Addr<UserSession>),
}

#[derive(Clone)]
pub struct AddressBook {
    pub signup_list: Addr<SignupListActor>,
}

pub struct Greeter {
    addrs: AddressBook,
}

impl Greeter {
    pub fn new(addrs: AddressBook) -> Self {
        Self { addrs }
    }
}

/// Perform any necessary onboarding tasks for a newly connected user
async fn onboard_user(addrs: AddressBook, user: Addr<UserSession>) -> anyhow::Result<()> {
    subscribe_user_to_signup_list(addrs.signup_list, user).await?;

    Ok(())
}

async fn subscribe_user_to_signup_list(
    signup_list: Addr<SignupListActor>,
    user: Addr<UserSession>,
) -> anyhow::Result<()> {
    let subscribe_msg = SubscribeToSignupList(user);
    signup_list
        .send(subscribe_msg)
        .await
        .context("sending subscribe message to signup list")?
        .context("response from signup list")?;

    Ok(())
}

impl Actor for Greeter {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("Started greeter");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("Stopped greeter");
    }
}

impl Handler<GreeterMessage> for Greeter {
    type Result = ();

    fn handle(&mut self, msg: GreeterMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            GreeterMessage::Hello(user) => {
                let addrs = self.addrs.clone();
                let fut = onboard_user(addrs, user).map(|res| {
                    res.context("onboarding user")
                        .map_err(|err| eprintln!("ERROR: {:?}", err))
                        .ok();
                });
                let actor_fut = fut.into_actor(self);
                ctx.spawn(actor_fut);
            }
        }
    }
}

pub fn start_greeter(addrs: AddressBook) -> Addr<Greeter> {
    let greeter = Greeter::new(addrs);
    greeter.start()
}
