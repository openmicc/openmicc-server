use actix::prelude::*;
use actix_web::error::Error as ActixError;
use actix_web::App;
use actix_web::{
    web::{get, Data, Payload},
    HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;
use tracing::{error, info, instrument};

use crate::greeter::Greeter;
use crate::user_session::UserSession;
use crate::utils::WrapAddr;

#[derive(Clone)]
pub struct AppData {
    pub greeter_addr: Addr<Greeter>,
}

#[instrument(skip(app_data))]
pub async fn run_http_server(port: u16, app_data: AppData) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    info!("Running on {}", &addr);

    error!("Here's an instrumented error!");

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(app_data.clone()))
            .route("/", get().to(ws_index))
            .route("/hello", get().to(hello))
    })
    .workers(2)
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}

async fn hello(
    _request: HttpRequest,
    _data: Data<AppData>,
    _stream: Payload,
) -> Result<HttpResponse, ActixError> {
    info!("Hello");
    let response = HttpResponse::Ok().body("Great job.".to_string());

    Ok(response)
}

/// Function that receives HTTP request on WebSocket route and upgrades it to WebSocket connection.
///
/// See https://actix.rs/docs/websockets/ for official `actix-web` documentation.
async fn ws_index(
    request: HttpRequest,
    data: Data<AppData>,
    stream: Payload,
) -> Result<HttpResponse, ActixError> {
    info!("ws_index");
    let actor = UserSession::new(data.greeter_addr.clone().wrap());

    ws::start(actor, &request, stream)

    // match EchoConnection::new(&worker_manager).await {
    //     Ok(echo_server) => {
    //         info!("Started echo server (upgrade WS)");
    //         ws::start(echo_server, &request, stream)
    //     }
    //     Err(error) => {
    //         error!("{}", error);

    //         Ok(HttpResponse::InternalServerError().finish())
    //     }
    // }
}
