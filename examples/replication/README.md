# Replication examples

These examples demonstrate the configurations and operations of a replication network using SwarmNL using Eventual Consistency and Strong Consistency models.

## Eventual consistency

To run this example, cd into the root of this directory and in separate terminals launch the following commands to launch three nodes:

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

Now, submit the following commands to replicate data from nodes in the network, starting with the first node:

```bash
repl Apples
```

The second node:

```bash
repl Oranges
```

Then the third node:

```bash
repl Papayas
```

Then in node 3, running the following command will return the values in its replication buffer (which contains data received from node 1 and 2):

```bash
read
```

## Run with Docker

Build:

```bash
docker build -t eventual-consistency ./eventual-consistency
```

Run:

```bash
docker run -it eventual-consistency
```

Hit `Ctrl+D` to exit.

## Strong consistency

To run this example, cd into the root of this directory and in separate terminals launch the following commands:

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

Now, submit the following commands to replicate data from nodes in the network, starting with the first node:

```bash
repl Apples
```

The second node:

```bash
repl Oranges
```

Then the third node:

```bash
repl Papayas
```

Then in node 3, running the following command will return the values in its replication buffer alongside the number of confirmations before the data was written to the primary public buffer and exposed to the application layer. 

```bash
read
```

## Run with Docker

Build:

```bash
docker build -t strong-consistency ./strong-consistency
```

Run:

```bash
docker run -it strong-consistency
```

Hit `Ctrl+D` to exit.

## Peer cloning

In this example, we expect a node to clone the data in the buffer of the specified replica peer when it calls `clone`.

To run this example, cd into the root of this directory and in separate terminals launch the following commands:

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

Now, submit the following commands to replicate data from nodes in the network, starting with the first node:

```bash
repl Apples
```

The second node:

```bash
repl Oranges
```

Then the third node:

```bash
repl Papayas
```

Then in node 3, run the following command to clone node 2's buffer (by passing in node 2's peer ID):

```bash
clone 12D3KooWFPuUnCFtbhtWPQk1HSGEiDxzVrQAxYZW5zuv2kGrsam4
```

We expect node 2 to contain "Papayas" and "Apples" in its buffer.
This can be verified by submitting `read` to stdin from node 3's terminal to read it's buffer content:

```bash
read
```

## Run with Docker

Build:

```bash
docker build -t peer-cloning ./peer-cloning
```

Run:

```bash
docker run -it peer-cloning
```

The Docker image will open three terminals and the script will submit commands to each one to demonstrate the example. To quit, use the `exit` command in each terminal.