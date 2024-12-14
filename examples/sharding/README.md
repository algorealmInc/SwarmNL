# Sharding examples

These examples demonstrate the sharding configurations and capabilities of SwarmNL using both hash-based and range-based sharding policies. 

## Hash-based sharding example

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

In the separate terminals where each node is running, submit the following commands:

```bash
# for node 1
shard mars mars_creatures.txt Boggles
# for node 2
shard earth earth_creatures.txt Unicorns
# for node 3
shard mars mars_creatures.txt Inkls
```

From node 3's terminal, you can read what is in node 3's buffer by submitting the `read` command. To fetch the values from the network, submit the following command from node 3:

```bash
fetch mars mars_creatures.txt
```

In node 2's terminal, run:

```bash
 fetch earth earth_creatures.txt
```

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

## Range-based sharding example

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

Once all nodes are running, submit the following commands to each terminal:

```bash
# in node 1's terminal
shard 150 song --> Give It Away

# in node 2's terminal
shard 250 song --> Under the Bridge

# in node 3's terminal
shard 55 song --> I Could Have Lied
```

Then use the `read` and `fetch` commands to check the networks behavior, by executing the following in the third terminal for example:

```bash
read && fetch 150 song
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