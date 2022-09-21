use crate::network::secure::generate_ed25519;
use crate::Mode;
use chrono::prelude::*;

use futures::executor::block_on;
use futures::prelude::*;
use instant::Duration;

use libp2p::core::multiaddr::{Multiaddr, Protocol};
use libp2p::core::transport::OrTransport;
use libp2p::core::upgrade;
use libp2p::{dcutr, Swarm};
use libp2p::dns::DnsConfig;
use libp2p::gossipsub::{
    self, GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, MessageId,
    ValidationMode,
};
use libp2p::identify::{Identify, IdentifyConfig, IdentifyEvent, IdentifyInfo};
use libp2p::noise;
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::relay::v2::client::{self, Client};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::tcp::{GenTcpConfig, TcpTransport};
use libp2p::yamux;
use libp2p::Transport;
use libp2p::{NetworkBehaviour, PeerId};

use log::info;
use tokio::sync::mpsc::{Receiver, Sender};
use std::collections::hash_map::DefaultHasher;
use std::convert::TryInto;
use std::hash::{Hash, Hasher};
use std::net::Ipv4Addr;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event", event_process = false)]
pub struct Behaviour {
    relay_client: Client,
    ping: Ping,
    identify: Identify,
    dcutr: dcutr::behaviour::Behaviour,
    pub gossip: gossipsub::Gossipsub,
}

#[derive(Debug)]
pub enum Event {
    Ping(PingEvent),
    Identify(IdentifyEvent),
    Relay(client::Event),
    Dcutr(dcutr::behaviour::Event),
    Gossip(GossipsubEvent),
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


pub async fn establish_connection(
    mode: &Mode,
    key: &String,
    relay_address: &Multiaddr,
    remote_id: &Option<PeerId>,
) -> Swarm<Behaviour> {
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

    // build swamr
    let mut swarm = {
        // use the hash of message as id to conetnt-address
        let message_id_fn = |message: &GossipsubMessage| {
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
        let mut gossip = gossipsub::Gossipsub::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
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

    // establish relay-connection with remote peer or request listening-connection to relay
    match *mode {
        Mode::Dial => {
            swarm
                .dial(
                    (*relay_address)
                        .clone()
                        .with(Protocol::P2pCircuit)
                        .with(Protocol::P2p(PeerId::from((*remote_id).unwrap()).into())),
                )
                .unwrap();
        }
        Mode::Listen => {
            swarm
                .listen_on((*relay_address).clone().with(Protocol::P2pCircuit))
                .unwrap();
        }
    }

    // waiting for connection to be established
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
    swarm

   
}

pub async fn receive(mut swarm: Swarm<Behaviour>, mut rx1: Receiver<String>, tx2: Sender<String>) {
    loop {
        let msg = rx1.recv().await.unwrap();
        swarm.behaviour_mut()
            .gossip
            .publish(Topic::new("abc"), msg.as_bytes())
            .expect("publish error");
        
        tokio::select! {
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::Behaviour(Event::Gossip(GossipsubEvent::Message{
                        propagation_source: _,
                        message_id: _,
                        message,
                    })) => {
                        let message = String::from_utf8_lossy(&message.data);
                        let tokens:Vec<&str> = message.split(",").collect();
                        let content = tokens[0];
                        let remote_name = tokens[1];

                        tx2.send(format!("{}, {}\r\n{}", 
                                    remote_name, 
                                    Local::now().format("%H:%M:%S").to_string(), 
                                    content)).await.unwrap();
                    }
                    _ => {}
                }
            }
        }
        
    }
}