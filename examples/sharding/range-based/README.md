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
docker build -t range-based .
```

Run:

```bash
docker run -it range-based
```

Hit `Ctrl+D` to exit.