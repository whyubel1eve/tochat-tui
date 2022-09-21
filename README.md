A **Distributed Peer-to-Peer instant messaging TUI application** built on **libp2p**

This program uses some protocols from libp2p to accomplish **Hole-Punching**, like `libp2p circuit relay v2`,  `libp2p Direct Connection Upgrade through Relay`. So there should be a public server(cloud server) to act as a relay.

## Installation

### 1. On public server:
```sh
cargo build --bin relay
cd tochat/target/debug/
nohup ./relay --port 4001 --secret-key-seed 0 &
```
Then watch the output and write down the listening address(with PeerId).
### 2. On both PCs 

For Ubuntu, some tools need to be pre-installed:
(other systems should download corresponding tools)

```sh
sudo apt install libssl-dev
sudo apt install protobuf-compiler
sudo apt install pkg-config
```
Then:
```sh
cargo build --bin tochat
cd tochat/target/debug
# create your secret key, and keep it
./tochat new
# someone should be listening first
RUST_LOG=info ./tochat start --mode listen --key `xxx` --name `xxx` --relay-address `xxx`
# another one dials
RUST_LOG=info ./tochat start --mode dial --key `xxx` --name `xxx` --remote-id `xxx` --relay-address `xxx`
# use `./tochat start --help` to check help details
```