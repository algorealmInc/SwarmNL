#!/bin/bash

# Start tmux session
tmux new-session -d -s rust-nodes "cargo run --features=first-node"
tmux split-window -h "cargo run --features=second-node"
tmux split-window -v "cargo run --features=third-node"

# Arrange panes for better layout
tmux select-layout tiled

# Give the nodes some time to start
sleep 10

# Send commands to each pane
# Pane 0 (first node)
tmux send-keys -t rust-nodes:0.0 "repl wonderful" C-m
sleep 3

# Pane 1 (second node)
tmux send-keys -t rust-nodes:0.1 "repl great" C-m
sleep 12

# Pane 2 (third node)
tmux send-keys -t rust-nodes:0.2 "repl amazing" C-m
sleep 3

# Attach to the session so you can observe the output
tmux attach-session -t rust-nodes