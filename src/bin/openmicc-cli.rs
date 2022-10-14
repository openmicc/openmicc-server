use std::ops::ControlFlow;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context as AnyhowContext};
use clap::{Parser, Subcommand};
use rustyline::error::ReadlineError;
use strum::{EnumDiscriminants, EnumIter, EnumString, IntoEnumIterator};
use tracing::{error, info};
use websocket::sync::client::ClientBuilder as WebSocketClientBuilder;
use websocket::OwnedMessage;

use openmicc_server::user_session::{ClientMessage, ServerMessage};
use openmicc_server::utils::LogOk;

type WebSocketClient = websocket::sync::Client<websocket::stream::sync::TcpStream>;
type WebSocketReader = websocket::sync::Reader<websocket::stream::sync::TcpStream>;
type WebSocketWriter = websocket::sync::Writer<websocket::stream::sync::TcpStream>;

#[derive(Parser)]
struct Opts {
    #[arg(default_value = "ws://localhost:3050")]
    url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Repl,
}

struct Client {
    ws: WebSocketClient,
}

struct ClientReadHalf {
    ws_rx: WebSocketReader,
}

impl ClientReadHalf {
    pub fn new(ws_rx: WebSocketReader) -> Self {
        Self { ws_rx }
    }

    pub fn print_incoming_messages(&mut self) -> anyhow::Result<()> {
        for msg_res in self.ws_rx.incoming_messages() {
            let raw_msg = msg_res?;
            match raw_msg {
                OwnedMessage::Text(text) => {
                    let msg: ServerMessage = serde_json::from_str(&text)
                        .with_context(|| format!("deserializing ServerMessage: '{}'", text))?;
                    // Print incoming messages
                    info!("RECV: {:?}", &msg);
                }
                OwnedMessage::Binary(_) => todo!(),
                OwnedMessage::Close(_) => todo!(),
                OwnedMessage::Ping(_) => todo!(),
                OwnedMessage::Pong(_) => todo!(),
            }
        }

        Ok(())
    }
}

impl ClientWriteHalf {
    pub fn new(ws_tx: WebSocketWriter) -> Self {
        Self { ws_tx }
    }

    /// Send a ClientMessage to the server
    fn send(&mut self, msg: ClientMessage) -> anyhow::Result<()> {
        let serialized = serde_json::to_string(&msg)?;
        let ws_msg = websocket::Message::text(serialized);
        self.ws_tx.send_message(&ws_msg)?;

        Ok(())
    }

    pub async fn signup(&mut self, name: &str) -> anyhow::Result<()> {
        let signup_msg = ClientMessage::SignMeUp(name.to_string().into());
        self.send(signup_msg)?;

        Ok(())
    }

    pub async fn get_list(&mut self) -> anyhow::Result<()> {
        let msg = ClientMessage::GetList;
        self.send(msg)?;

        Ok(())
    }
}

struct ClientWriteHalf {
    ws_tx: WebSocketWriter,
}

impl Client {
    pub fn try_new(url: &str) -> anyhow::Result<Self> {
        let ws = WebSocketClientBuilder::new(url)?.connect_insecure()?;
        let new = Self { ws };

        Ok(new)
    }

    pub fn split(self) -> anyhow::Result<(ClientReadHalf, ClientWriteHalf)> {
        let (ws_rx, ws_tx) = self.ws.split()?;
        let client_rx = ClientReadHalf::new(ws_rx);
        let client_tx = ClientWriteHalf::new(ws_tx);

        Ok((client_rx, client_tx))
    }
}

async fn run_repl_tx(mut client_tx: ClientWriteHalf) -> anyhow::Result<()> {
    let mut repl = rustyline::Editor::<()>::new().context("creating repl")?;

    'repl: loop {
        let readline = repl.readline("> ");

        match readline {
            Ok(line) => {
                if let Some(cmd_res) = ReplCommand::parse(&line) {
                    let maybe_command = cmd_res
                        .with_context(|| anyhow!(ReplCommand::help_message()))
                        .context("parsing repl command")
                        .ok_log_err();

                    if let Some(command) = maybe_command {
                        let maybe_control_flow = handle_command(&mut client_tx, command)
                            .await
                            .context("handling command")
                            .ok_log_err();

                        if let Some(control_flow) = maybe_control_flow {
                            match control_flow {
                                ControlFlow::Continue(..) => {
                                    println!();
                                }
                                ControlFlow::Break(..) => {
                                    break 'repl;
                                }
                            }
                        }
                    }
                }
            }
            // Err(ReadlineError::Interrupted) => {
            //     error!("CTRL-C");
            //     break;
            // }
            Err(ReadlineError::Eof) => {
                error!("CTRL-D");
                break;
            }
            Err(err) => {
                error!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

async fn run_repl_rx(mut client_tx: ClientReadHalf) -> anyhow::Result<()> {
    client_tx.print_incoming_messages()?;

    Ok(())
}

async fn run_repl(client: Client) -> anyhow::Result<()> {
    let (client_rx, client_tx) = client.split()?;

    let rx_fut = tokio::task::spawn(run_repl_rx(client_rx));
    let tx_fut = tokio::task::spawn(run_repl_tx(client_tx));

    let (rx_res, tx_res) = tokio::try_join!(rx_fut, tx_fut)?;
    rx_res?;
    tx_res?;
    // tx_fut.await?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();

    let client = Client::try_new(&opts.url)?;

    match opts.command {
        Command::Repl => run_repl(client).await?,
    }

    info!("exiting.");

    Ok(())
}

#[derive(Debug, EnumDiscriminants)]
#[strum_discriminants(derive(EnumString, EnumIter, strum::Display))]
#[strum_discriminants(strum(serialize_all = "lowercase"))]
enum ReplCommand {
    /// Exit the REPL.
    Exit,
    /// Put my name on the list.
    SignUp { name: String },
    /// Get the current signup list
    List,
}

impl ReplCommand {
    pub fn help_message() -> String {
        let mut buf = String::new();
        buf.push_str("valid commands:\n");
        for cmd in ReplCommandDiscriminants::iter() {
            buf.push_str(&format!("- {}\n", cmd));
        }

        buf
    }

    /// Parse a string into a command
    pub fn parse(cmd_str: &str) -> Option<anyhow::Result<Self>> {
        let words: Vec<&str> = cmd_str.split_whitespace().collect();
        if let Some((first, rest)) = words.split_first() {
            // At least one word given
            Some(Self::parse_inner(first, rest))
        } else {
            // Empty command
            None
        }
    }

    fn parse_inner(first: &str, rest: &[&str]) -> anyhow::Result<Self> {
        use ReplCommandDiscriminants::*;
        let discriminant =
            ReplCommandDiscriminants::from_str(&first).context("parsing first word in command")?;
        match discriminant {
            Exit => match rest.len() {
                0 => Ok(ReplCommand::Exit),
                _ => bail!("`exit` takes no args"),
            },
            SignUp => match rest.split_first() {
                Some((name, &[])) => Ok(ReplCommand::SignUp {
                    name: name.to_string(),
                }),
                Some(..) => bail!("too many args given to `signup`"),
                None => bail!("`signup` requires one arg: `name`"),
            },
            List => match rest.len() {
                0 => Ok(ReplCommand::List),
                _ => bail!("`list` takes no args"),
            },
        }
    }
}

async fn handle_command(
    client: &mut ClientWriteHalf,
    cmd: ReplCommand,
) -> anyhow::Result<ControlFlow<()>> {
    let res = match cmd {
        ReplCommand::Exit => ControlFlow::Break(()),
        ReplCommand::SignUp { name } => {
            client.signup(&name).await.context("signing up")?;
            ControlFlow::Continue(())
        }
        ReplCommand::List => {
            client.get_list().await.context("getting list")?;
            ControlFlow::Continue(())
        }
    };

    Ok(res)
}
