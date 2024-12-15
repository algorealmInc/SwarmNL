# Simple game example

This example runs a simple guessing game between two nodes. The game's logic is as follows:

- Node 1 and Node 2 connect to each other
- Then at an interval, they each guess a value
- The value is gossiped to the other peer
- The incoming value is then compared with the local value
- If the local value is greater, a score count is increased
- Whoever gets to the HIGHT_SCORE first wins the game and gossips the result
- The result is exchanged and the winner is announced on both nodes
- Once the winner is announced, the game ends and both nodes exit

We filter every guess that is greater than 10, and each guess that is greater than 10 is dropped at both ends. All other guesses are gossiped to the network.

Guesses that are greater than 10 on arrival will not be propagated to the application for handling, but will be dropped at the application layer on arrival because of the filtering logic that's implemented.

> This example uses `tokio-runtime`, specified in the crate's Cargo.toml file. You can also run this example using `async-std-runtime` by updating your Cargo.toml file accordingly.

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

## Run with Docker

Build:

```bash
docker build -t simple-game-demo .
```

Run:

```bash
docker run -it simple-game-demo
```
