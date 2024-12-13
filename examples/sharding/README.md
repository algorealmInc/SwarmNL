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

## Run with Docker

Build:

```bash
docker build -t hash-based ./hash-based
```

Run:

```bash
docker run -it hash-based
```

Hit `Ctrl+D` to exit.

# Range-based sharding example

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