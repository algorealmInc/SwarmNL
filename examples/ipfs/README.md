# IPFS example

This example demonstrates how to interact with IPFS in response to incoming replication data events, allowing you to store or retrieve data from the IPFS network dynamically.

To run this example, **make sure you are running an IPFS daemon locally** and launch the following commands from the root of this directory:

```bash
cargo run --features=first-node
```
 
Then the second node:

```bash
cargo run --features=second-node
```

And the third node:

```bash
cargo run --features=third-node
```

In the separate terminals where each node is running, submit the following commands:

```bash
# for node 1
repl Apples
# for node 2
repl Oranges
# for node 3
repl Papayas
```
When the replicated data arrived on a node and reaches the application layer, it is immediately uploaded to the IPFS network for persistence.
After the operation, you should see a message printed to the stdout of each node such as:

```bash
File successfully uploaded to IPFS with hash: QmX7epzCn2jD8nPUDiehmZDQs69HxKYcmM
```

## Run with Docker

Build:

```bash
docker build -t ipfs-example ./ipfs
```

Run:

```bash
docker run -it ipfs-example
```

The Docker image will run an IPFS daemon, open three terminals and the script will submit `repl` commands to each one to demonstrate the example. To quit, use the `exit` command in each terminal.