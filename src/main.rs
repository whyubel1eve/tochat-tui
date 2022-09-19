// Copyright 2021 Protocol Labs.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use clap::{Parser, Subcommand};
use async_std::io;
use async_std::task::block_on;
use futures::{prelude::*, select};
use instant::Duration;
use libp2p::core::multiaddr::{Multiaddr, Protocol};
use libp2p::core::transport::OrTransport;
use libp2p::core::upgrade;
use libp2p::dcutr;
use libp2p::dns::DnsConfig;
use libp2p::identify::{Identify, IdentifyConfig, IdentifyEvent, IdentifyInfo};
use libp2p::identity::Keypair;
use libp2p::identity::ed25519::SecretKey;
use libp2p::noise;
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::relay::v2::client::{self, Client};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::tcp::{GenTcpConfig, TcpTransport};
use libp2p::Transport;
use libp2p::gossipsub::{
    self, GossipsubEvent, MessageId, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::yamux;
use libp2p::{identity, NetworkBehaviour, PeerId};
use log::info;
use std::collections::hash_map::DefaultHasher;
use std::convert::TryInto;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::net::Ipv4Addr;
use std::str::FromStr;
use colorful::Colorful;
use colorful::Color;
use chrono::prelude::*;
use web3::signing::keccak256;
use secp256k1::rand::rngs::OsRng;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands{
    /// Create a new private key
    New,
    Start{
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
        #[clap(long, default_value = "/ip4/1.12.76.121/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN")]
        relay_address: Multiaddr,

        /// ID of the remote peer to hole punch to.
        #[clap(long)]
        remote_id: Option<PeerId>,
    }
}

#[derive(Debug, Parser, PartialEq)]
enum Mode {
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

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::New => {
            let secret_key = secp256k1::SecretKey::new(&mut OsRng);
            println!("{}", "The secret_key only shows once!!! Please keep it safe.\n".color(Color::LightRed));
            println!("{}", secret_key.display_secret());
            Ok(())
        }
        Commands::Start { mode, key, name, relay_address, remote_id } => {

            let local_key = generate_ed25519(key);

            let local_peer_id = PeerId::from(local_key.public());
            info!("Local peer id: {:?}", local_peer_id);

            let (relay_transport, client) = Client::new_transport_and_behaviour(local_peer_id);

            let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
                .into_authentic(&local_key)
                .expect("Signing libp2p-noise static DH keypair failed.");

            let transport = OrTransport::new(
                relay_transport,
                block_on(DnsConfig::system(TcpTransport::new(
                    GenTcpConfig::default().port_reuse(true),
                )))
                .unwrap(),
            )
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
            .multiplex(yamux::YamuxConfig::default())
            .boxed();

            let topic = Topic::new("abc");

            #[derive(NetworkBehaviour)]
            #[behaviour(out_event = "Event", event_process = false)]
            struct Behaviour {
                relay_client: Client,
                ping: Ping,
                identify: Identify,
                dcutr: dcutr::behaviour::Behaviour,
                gossip: gossipsub::Gossipsub,
            }

            #[derive(Debug)]
            enum Event {
                Ping(PingEvent),
                Identify(IdentifyEvent),
                Relay(client::Event),
                Dcutr(dcutr::behaviour::Event),
                Gossip(GossipsubEvent)
            }

            impl From<PingEvent> for Event {
                fn from(e: PingEvent) -> Self {
                    Event::Ping(e)
                }
            }

            impl From<IdentifyEvent> for Event {
                fn from(e: IdentifyEvent) -> Self {
                    Event::Identify(e)
                }
            }

            impl From<client::Event> for Event {
                fn from(e: client::Event) -> Self {
                    Event::Relay(e)
                }
            }

            impl From<dcutr::behaviour::Event> for Event {
                fn from(e: dcutr::behaviour::Event) -> Self {
                    Event::Dcutr(e)
                }
            }
            impl From<GossipsubEvent> for Event {
                fn from(e: GossipsubEvent) -> Self {
                    Event::Gossip(e) 
                }
            }


            let mut swarm = {
                // use the hash of message as id to conetnt-address
                let message_id_fn = | message: &GossipsubMessage | {
                    let mut s = DefaultHasher::new();
                    message.data.hash(&mut s);
                    MessageId::from(s.finish().to_string())
                };
                // set a custom gossipsub
                let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                    .validation_mode(ValidationMode::Strict) // set the message validation. Enforce message validation
                    .message_id_fn(message_id_fn) // content-address. not to propagate same content messages
                    .build()
                    .expect("Valid config");
                let mut gossip = gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key.clone()), gossipsub_config)
                    .expect("configuration error");

                gossip.subscribe(&topic).unwrap();

                let behaviour = Behaviour {
                    relay_client: client,
                    ping: Ping::new(PingConfig::new()),
                    identify: Identify::new(IdentifyConfig::new(
                        "/TODO/0.0.1".to_string(),
                        local_key.public(),
                    )),
                    dcutr: dcutr::behaviour::Behaviour::new(),
                    gossip,
                };
                SwarmBuilder::new(transport, behaviour, local_peer_id)
                .dial_concurrency_factor(10_u8.try_into().unwrap())
                .build()
            };

            swarm
                .listen_on(
                    Multiaddr::empty()
                        .with("0.0.0.0".parse::<Ipv4Addr>().unwrap().into())
                        .with(Protocol::Tcp(0)),
                )
                .unwrap();

            // Wait to listen on all interfaces.
            block_on(async {
                let mut delay = futures_timer::Delay::new(std::time::Duration::from_secs(1)).fuse();
                loop {
                    futures::select! {
                        event = swarm.next() => {
                            match event.unwrap() {
                                SwarmEvent::NewListenAddr { address, .. } => {
                                    info!("Listening on {:?}", address);
                                }
                                event => panic!("{:?}", event),
                            }
                        }
                        _ = delay => {
                            // Likely listening on all interfaces now, thus continuing by breaking the loop.
                            break;
                        }
                    }
                }
            });

            // Connect to the relay server. Not for the reservation or relayed connection, but to (a) learn
            // our local public address and (b) enable a freshly started relay to learn its public address.
            swarm.dial((*relay_address).clone()).unwrap();
            block_on(async {
                let mut learned_observed_addr = false;
                let mut told_relay_observed_addr = false;

                loop {
                    match swarm.next().await.unwrap() {
                        SwarmEvent::NewListenAddr { .. } => {}
                        SwarmEvent::Dialing { .. } => {}
                        SwarmEvent::ConnectionEstablished { .. } => {}
                        SwarmEvent::Behaviour(Event::Gossip(_)) => {}
                        SwarmEvent::Behaviour(Event::Ping(_)) => {}
                        SwarmEvent::Behaviour(Event::Identify(IdentifyEvent::Sent { .. })) => {
                            info!("Told relay its public address.");
                            told_relay_observed_addr = true;
                        }
                        SwarmEvent::Behaviour(Event::Identify(IdentifyEvent::Received {
                            info: IdentifyInfo { observed_addr, .. },
                            ..
                        })) => {
                            info!("Relay told us our public address: {:?}", observed_addr);
                            learned_observed_addr = true;
                        }
                        event => panic!("{:?}", event),
                    }

                    if learned_observed_addr && told_relay_observed_addr {
                        break;
                    }
                }
            });

            match *mode {
                Mode::Dial => {
                    swarm
                        .dial(
                            (*relay_address).clone()
                                .with(Protocol::P2pCircuit)
                                .with(Protocol::P2p(PeerId::from((*remote_id).unwrap()).into()))
                        )
                        .unwrap();
                }
                Mode::Listen => {
                    swarm
                        .listen_on((*relay_address).clone().with(Protocol::P2pCircuit))
                        .unwrap();
                }
            }

            block_on(async {
                let mut established = false;
                loop {
                    match swarm.next().await.unwrap() {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Listening on {:?}", address);
                    }
                    SwarmEvent::Behaviour(Event::Relay(client::Event::ReservationReqAccepted {
                        ..
                    })) => {
                        assert!(*mode == Mode::Listen);
                        info!("Relay accepted our reservation request.");
                    }
                    SwarmEvent::Behaviour(Event::Relay(event)) => {
                        info!("{:?}", event)
                    }
                    SwarmEvent::Behaviour(Event::Dcutr(event)) => {
                        info!("{:?}", event);
                        established = true;
                    }
                    SwarmEvent::Behaviour(Event::Identify(event)) => {
                        info!("{:?}", event)
                    }
                    SwarmEvent::Behaviour(Event::Ping(_)) => {}
                    SwarmEvent::ConnectionEstablished {
                        peer_id, endpoint, ..
                    } => {
                        info!("Established connection to {:?} via {:?}", peer_id, endpoint);
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error } => {
                        info!("Outgoing connection error to {:?}: {:?}", peer_id, error);
                    }
                    _ => {}
                    }

                    if established {
                        break;
                    }
                }
            });

            let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
            loop {
                select! {
                    line = stdin.select_next_some() => {
                        if let Err(e) = swarm
                            .behaviour_mut()
                            .gossip
                            .publish(topic.clone(), format!("{},{}", line.expect("Stdin not to close"), name)
                                .as_bytes())
                        {
                            println!("Publish error: {:?}", e);
                        }
                    },
                    event = swarm.select_next_some() => match event {
                        SwarmEvent::Behaviour(Event::Gossip(GossipsubEvent::Message{
                            propagation_source: _,
                            message_id: _,
                            message,
                        })) => {
                            let message = String::from_utf8_lossy(&message.data);
                            let tokens:Vec<&str> = message.split(",").collect();
                            let content = tokens[0];
                            let remote_name = tokens[1];
                                 
                            println!(
                            "{} : {}",
                            format!("{}  {}", remote_name, Local::now().format("%H:%M:%S").to_string())
                                .color(Color::LightCyan),
                            content.color(Color::LightYellow),
                            );
                            print!("{}  {} : ", name, Local::now().format("%H:%M:%S").to_string()
                                .color(Color::LightCyan))
                        }, 
                        _ => {}
                    }
                }
            }
        }
    }
}

fn generate_ed25519(key: &String) -> identity::Keypair {
    let mut hash = keccak256(key.as_bytes());
    let secret_key = SecretKey::from_bytes(&mut hash).unwrap();
    Keypair::Ed25519(secret_key.into())
}
