use actix::dev::ToEnvelope;
use actix::prelude::*;
use actix::Actor;

pub async fn send_or_log_err_inner<A, M>(addr: Addr<A>, msg: M)
where
    A: Actor,
    A::Context: ToEnvelope<A, M>,
    M: std::fmt::Debug + Message + Send + 'static,
    M::Result: std::fmt::Debug + Send,
    A: Handler<M>,
{
    println!("SEND MESSAGE {:?}", &msg);
    match addr.send(msg).await {
        Ok(res) => println!("Send result {:?}", res),
        Err(err) => eprintln!("ERROR 1: {:?}", err),
    }
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
