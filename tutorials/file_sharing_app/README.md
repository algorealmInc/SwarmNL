# File sharing example

This example involves running two nodes:

- One node to write a record to the DHT and specify itself as a provider for a file it has locally
- Another node to read the DHT

The app then uses an RPC to fetch the file from the first peer and prints it in the terminal where the second node is running.
A 5 second timeout is set to give enough time for the nodes to communicate with eachother.

> This example uses `async-std-runtime` specified in the crate's Cargo.toml file to demonsrate SwarmNl's compatibility with using the async-std runtime.

**Note:** The example requires a quorum of 1, which means that that if the second node does not run the first node will not be able to write to the DHT and will panic.

## Run the example

To run this example, you'll need two terminals.

1. In the first terminal, cd into the root of this directory and run:

```bash
cargo run --features=first-node
```

1. In the second terminal, cd into the root of this directory and _immediately_ run (there's a 5 second timeout after which the first node will panic if it doesn't connect to the second node):

```bash
cargo run --features=second-node
```

In your first terminal, you should see the an output similar to this:

```bash
Peer id: 12D3KooWCHqiBVTsUDy4ZtcV2Ds7rxt2HGuaT5dHTKskw68Y8AWu
We're listening on the /ip4/127.0.0.1/tcp/49666
Peer id: 12D3KooWCHqiBVTsUDy4ZtcV2Ds7rxt2HGuaT5dHTKskw68Y8AWu
We're listening on the /ip4/192.168.178.88/tcp/49666
Peer id: 12D3KooWCHqiBVTsUDy4ZtcV2Ds7rxt2HGuaT5dHTKskw68Y8AWu
We're listening on the /ip4/127.0.0.1/udp/49606/quic-v1
Peer id: 12D3KooWCHqiBVTsUDy4ZtcV2Ds7rxt2HGuaT5dHTKskw68Y8AWu
We're listening on the /ip4/192.168.178.88/udp/49606/quic-v1
Connection established with peer: PeerId("12D3KooWPHwTgtTvmR2evoyQFFi9v7xtiAsVWBdgSbJ1muub1kmj")
Record successfully written to DHT. Key: [99, 111, 110, 102, 105, 103, 95, 102, 105, 108, 101]
```

Shortly after you ran the second node, you should see that the RPC has been received by the first node.

In the second terminal, you should see something similar to:

```bash
File read from DHT: bootstrap_config.ini
A fetch request has been sent to peer: PeerId("12D3KooWCHqiBVTsUDy4ZtcV2Ds7rxt2HGuaT5dHTKskw68Y8AWu")
```

And the second terminal will print the contents of the file: 

```bash
Here is the file delivered from the remote peer:

; Copyright (c) 2024 Algorealm
; A typical template showing the various configurations for bootstraping a node

; If this section is missing, the default ports will be used upon node setup
[ports]
; TCP/IP port to listen on
tcp=3000
; UDP port to listen on
udp=4000

; This section is for the node's identity and cryptographic keypair
; If this section is missing, a Ed25519 keypair will be generated upon node setup
[auth]
; Type of keypair to generate for node identity and message auth e.g RSA, EDSA, Ed25519
crypto=Ed25519
; The protobuf serialized format of the node's cryptographic keypair
protobuf_keypair=[]

[bootstrap]
; The boostrap nodes to connect to immediately after start up
boot_nodes=[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/x.x.x.x/tcp/1509, QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt:/ip4/x.x.x.x/tcp/1509]

[blacklist]
; The list of blacklisted peers we don't want to have anything to do with
blacklist=[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq, QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt]
```

## Run with Docker

Build:

```bash
docker build -t file-sharing-demo .
```

Run:

```bash
docker run -it file-sharing-demo
```
