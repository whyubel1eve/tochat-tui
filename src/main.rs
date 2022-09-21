mod network;
mod tui;

use clap::{Parser, Subcommand};

use libp2p::core::multiaddr::Multiaddr;

use libp2p::PeerId;

use std::error::Error;

use std::str::FromStr;
use tokio::sync::mpsc;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Create a new private key
    New,
    Start {
        /// The mode (client-listen, client-dial).
        #[clap(long)]
        mode: Mode,

        /// Fixed value to generate deterministic peer id.
        #[clap(long)]
        key: String,

        /// nickname
        #[clap(long)]
        name: String,

        /// The listening address
        #[clap(
            long,
            default_value = "/ip4/1.12.76.121/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
        )]
        relay_address: Multiaddr,

        /// ID of the remote peer to hole punch to.
        #[clap(long)]
        remote_id: Option<PeerId>,
    },
}

#[derive(Debug, Parser, PartialEq)]
pub enum Mode {
    Dial,
    Listen,
}

impl FromStr for Mode {
    type Err = String;
    fn from_str(mode: &str) -> Result<Self, Self::Err> {
        match mode {
            "dial" => Ok(Mode::Dial),
            "listen" => Ok(Mode::Listen),
            _ => Err("Expected either 'dial' or 'listen'".to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::New => network::secure::new_secret_key(),
        Commands::Start {
            mode,
            key,
            name,
            relay_address,
            remote_id,
        } => {  
            
            let (tx1, rx1) = mpsc::channel::<String>(32);
            let (tx2, rx2) = mpsc::channel::<String>(32);

            let swarm = network::connection::establish_connection(mode, key, relay_address, remote_id).await;
            tokio::spawn(network::connection::handle_msg(swarm, rx1, tx2));
            tui::bootstrap(tx1, rx2, name).await.unwrap();
            Ok(())
        }
    }
}
