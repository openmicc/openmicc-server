use std::fmt::Write;
use std::ops::ControlFlow;
use std::str::FromStr;

use anyhow::{anyhow, Context as AnyhowContext};
use clap::{Parser, Subcommand};
use openmicc_server::user_session::ServerMessage;
use rustyline::error::ReadlineError;
use serde::Deserialize;
use strum::{EnumIter, EnumString, IntoEnumIterator};
use websocket::sync::client::ClientBuilder as WebSocketClientBuilder;
use websocket::OwnedMessage;

type WebSocketClient = websocket::sync::Client<websocket::stream::sync::TcpStream>;

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
    Repl,
}

async fn run_repl(mut client: WebSocketClient) -> anyhow::Result<()> {
    let mut repl = rustyline::Editor::<()>::new().context("creating repl")?;

    'repl: loop {
        let readline = repl.readline("> ");

        match readline {
            Ok(line) => match ReplCommand::from_str(&line)
                .context("parsing repl command")
                .map_err(|_| anyhow!(ReplCommand::help_message()))
            {
                Err(err) => eprintln!("{:#}", err),
                Ok(command) => match handle_command(&mut client, command) {
                    Ok(ControlFlow::Continue(..)) => {}
                    Ok(ControlFlow::Break(..)) => {
                        break 'repl;
                    }
                    Err(err) => {
                        eprintln!("{:#}", err);
                    }
                },
            },
            // Err(ReadlineError::Interrupted) => {
            //     eprintln!("CTRL-C");
            //     break;
            // }
            Err(ReadlineError::Eof) => {
                eprintln!("CTRL-D");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
        println!();
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();

    let client = WebSocketClientBuilder::new(&opts.url)?.connect_insecure()?;

    match opts.command {
        Command::GetList => todo!(),
        Command::Repl => run_repl(client).await?,
    }

    println!("exiting.");

    Ok(())
}

fn print_incoming_messages(mut client: WebSocketClient) -> anyhow::Result<()> {
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
    }

    Ok(())
}

#[derive(EnumString, EnumIter, strum::Display)]
#[strum(serialize_all = "lowercase")]
enum ReplCommand {
    Exit,
}

impl ReplCommand {
    fn help_message() -> String {
        let mut buf = String::new();
        buf.push_str("valid commands:\n");
        for cmd in ReplCommand::iter() {
            buf.push_str(&format!("- {}\n", cmd));
        }

        buf
    }
}

fn handle_command(
    client: &mut WebSocketClient,
    cmd: ReplCommand,
) -> anyhow::Result<ControlFlow<()>> {
    let res = match cmd {
        ReplCommand::Exit => ControlFlow::Break(()),
    };

    Ok(res)
}
