mod network;
mod tui;

use clap::{Parser, Subcommand};
use libp2p::core::multiaddr::Multiaddr;
use libp2p::PeerId;
use tokio::sync::mpsc;
use std::error::Error;

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
    /// Import your secret key
    Import {
         /// secret key
         #[clap(long)]
         key: String,
    },
    /// Direct Message
    DM {
        /// nickname
        #[clap(long)]
        name: String,

        /// chat topic 
        #[clap(long)]
        topic: String,

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


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::New => network::secure::new_secret_key(),
        Commands::Import { key } => network::secure::import_secret(key),
        Commands::DM {
            name,
            topic,
            relay_address,
            remote_id,
        } => {  
            let key = network::secure::get_secret();
            
            let (tx1, rx1) = mpsc::channel::<String>(32);
            let (tx2, rx2) = mpsc::channel::<String>(32);

            let swarm = network::connection::establish_connection(&key, topic, relay_address, remote_id).await;
            tokio::spawn(network::connection::handle_msg(swarm, rx1, tx2, topic.clone()));
            tui::bootstrap(tx1, rx2, name).await.unwrap();
            Ok(())
        }
    }
}