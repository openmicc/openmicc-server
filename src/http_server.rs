use actix::prelude::*;

use actix_web::error::Error as ActixError;
use actix_web::App;
use actix_web::{
    web::{get, Data, Payload},
    HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;

use crate::greeter::Greeter;
use crate::user_session::UserSession;

#[derive(Clone)]
pub struct AppData {
    pub greeter_addr: Addr<Greeter>,
}

pub async fn run_http_server(port: u16, app_data: AppData) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    println!("Running on {}", &addr);

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
    println!("Hello");
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
    println!("ws_index");
    let actor = UserSession::new(data.greeter_addr.clone());

    ws::start(actor, &request, stream)

    // match EchoConnection::new(&worker_manager).await {
    //     Ok(echo_server) => {
    //         println!("Started echo server (upgrade WS)");
    //         ws::start(echo_server, &request, stream)
    //     }
    //     Err(error) => {
    //         eprintln!("{}", error);

    //         Ok(HttpResponse::InternalServerError().finish())
    //     }
    // }
}
