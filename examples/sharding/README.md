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

According to the configuration in the example, node 1 and 2 belongs to the shard with key "mars". Node 3 beloings to a separate shard with key "earth".
To read the local data stored on node 1 (mars shard). Run the following command:

```bash
read
```

After that, we would query the "earth" shard for the data it holds. To do that, please run the following command:

```bash
fetch earth earth_creatures.txt
```

Here, we are sending a data request to the sharded network, telling it to read the file "earth_creatures.txt" on the shard "earth".

From node 3's terminal, you can also read what is stored in node 3 by submitting the `read` command. To request data stored in the "mars" shard, kindly run the following:

```bash
fetch mars mars_creatures.txt
```

In node 2's terminal, you can also run the following:

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
# in node 1's terminal (stores it in shard 2 (node 2))
shard 150 song --> Give It Away

# in node 2's terminal (stores it in shard 1 (node 1))
shard 250 song --> Under the Bridge

# in node 3's terminal (stores it in shard 3 (node 3))
shard 55 song --> I Could Have Lied
```

To read data stored locally on a particular node, run the following command:

```bash
read
```
Please note that once read is called, all the available data is removed from the replica buffer and consumed.

TO fetch a song placed in a particular shard, please run the following:

```bash
# Run this in node 1's terminal
fetch 150 song
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