use anyhow::Context as AnyhowContext;
use clap::{Parser, Subcommand};
use openmicc_server::user_session::ServerMessage;
use websocket::{sync::client::ClientBuilder as WebsocketClientBuilder, OwnedMessage};

#[derive(Parser)]
struct Opts {
    #[arg(default_value = "ws://localhost:3050")]
    url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    GetList,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();

    let mut client = WebsocketClientBuilder::new(&opts.url)?.connect_insecure()?;

    for msg_res in client.incoming_messages() {
        let raw_msg = msg_res?;
        match raw_msg {
            OwnedMessage::Text(text) => {
                let msg: ServerMessage = serde_json::from_str(&text)
                    .with_context(|| format!("deserializing ServerMessage: '{}'", text))?;
                println!("Got text message: {:?}", &msg);
            }
            OwnedMessage::Binary(_) => todo!(),
            OwnedMessage::Close(_) => todo!(),
            OwnedMessage::Ping(_) => todo!(),
            OwnedMessage::Pong(_) => todo!(),
        }
        // let msg =
    }

    println!("connected.");

    Ok(())
}
