# Echo Server example

To run this example, cd into the root of the repository and run:

```bash
cargo run
```
 
Then submit an input into the terminal and watch your input get echoed back to you.

## Run with Docker

Build:

```bash
docker build -t echo-server .
```

Run:

```bash
docker run -it echo-server
```

Then submit an input into the terminal and watch your input get echoed back to you.

Hit `Ctrl+D` to exit.