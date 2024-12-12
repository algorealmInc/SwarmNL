# Hash-based sharding example

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

You should see that node 1 and 2 will join the same shard and node 3 will join a different shard.

To request data from the network

## Run with Docker

Build:

```bash
docker build -t echo-server .
```

Run:

```bash
docker run -it echo-server
```

Hit `Ctrl+D` to exit.