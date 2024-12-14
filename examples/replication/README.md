# Replication examples

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

Then in node 3, running the following command will return the values in its replication buffer:

```bash
read
```

## Run with Docker

Build:

```bash
docker build -t eventual-consistency ./eventual_consistency
```

Run:

```bash
docker run -it eventual-consistency
```

Hit `Ctrl+D` to exit.

## Peer cloning

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
cargo run --features=first-node
```

## Run with Docker

Build:

```bash
docker build -t range-based ./range-based
```

Run:

```bash
docker run -it range-based
```

Hit `Ctrl+D` to exit.

## Peer cloning

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
cargo run --features=first-node
```

## Run with Docker

Build:

```bash
docker build -t range-based ./range-based
```

Run:

```bash
docker run -it range-based
```

The Docker image will open three terminals and the script will submit commands to each one to demonstrate the example. To quit, use the `exit` command in each terminal.