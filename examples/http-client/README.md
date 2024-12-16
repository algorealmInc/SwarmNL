# HTTP client example

SwarmNL supports seamless integration with HTTP clients working in the application layer by providing interfaces to handle network events as they occur. This example demonstrates how to send an HTTP POST request to a remote server in response to incoming replication data events.

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

Replicate data across the network:

```bash
# in the first terminal
repl wonderful

# in the second terminal
repl great

# in the third terminal
repl amazing
```

When the replicated data arrives on a node and reaches the application layer, it is immediately sent to a remote server.
You should see the message `Successfully sent POST request.` printed to the terminal afte this operation.

## Run with Docker

Build:

```bash
docker build -t http-client .
```

Run:

```bash
docker run -it http-client
```

The Docker image will open three terminals and the script will submit commands to each one to demonstrate the example. To quit, use the `exit` command in each terminal.