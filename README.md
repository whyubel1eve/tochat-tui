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
### 2. On clients 

For Ubuntu, some tools need to be pre-installed:
(other systems should download corresponding tools)

```sh
sudo apt install libssl-dev
sudo apt install protobuf-compiler
sudo apt install pkg-config
```
build:
```sh
cargo build --bin tochat
cd tochat/target/debug
```
#### Direct Message:
```sh
# create your secret key or import an existed key
./tochat new || ./tochat import --key `xxx`
# someone should be listening first
./tochat dm --name `xxx` --relay-address `xxx` --topic `xxx`
# another one dials
./tochat dm --name `xxx` --relay-address `xxx` --remote-id `xxx` --topic `xxx`

# use `./tochat dm --help` to check help details
```
#### Group Message:

```sh
# create your secret key or import an existed key
./tochat new || ./tochat import --key `xxx`
# join the group 
./tochat channel --name `xxx` --relay-address `xxx` --topic `xxx`

# use `./tochat channel --help` to check help details
```