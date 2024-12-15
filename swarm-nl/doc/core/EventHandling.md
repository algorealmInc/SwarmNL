# Event Handling

Network events are stored in an internal buffer in the network layer as they occur. They have to be polled and consumed explicitly and are exposed by [`Core`] to the application layer by two functions:

- [`Core::next_event()]: This function returns the next event in the queue and returns `None` if the internal event buffer queue is empty.

```rust
    // ...
    // Read events generated at setup
	while let Some(event) = node.next_event().await {
		match event {
			NetworkEvent::NewListenAddr {
				local_peer_id,
				listener_id: _,
				address,
			} => {
				// Announce interfaces we're listening on
				println!("Peer id: {}", local_peer_id);
				println!("We're listening on {}", address);
			},
			NetworkEvent::ConnectionEstablished {
				peer_id,
				connection_id: _,
				endpoint: _,
				num_established: _,
				established_in: _,
			} => {
				println!("Connection established with peer: {:?}", peer_id);
			},
			_ => {},
		}
	}
    // ...
```

- [`Core::events()`]: This function drains all the buffered events and returns it to the application layer as an `Iterator` object;

```rust
    // ...
    let _ = events
		.map(|e| {
			match e {
				NetworkEvent::NewListenAddr {
					local_peer_id,
					listener_id: _,
					address,
				} => {
					// Announce interfaces we're listening on
					println!("[[Node {}]] >> Peer id: {}", game_state.node, local_peer_id);
					println!(
						"[[Node {}]] >> We're listening on the {}",
						game_state.node, address
					);
				},
				NetworkEvent::ConnectionEstablished {
					peer_id,
					connection_id: _,
					endpoint: _,
					num_established: _,
					established_in: _,
				} => {
					println!("Connection established with peer: {:?}", peer_id);
				},
				_ => {},
			}
		})
		.collect::<Vec<_>>();
    // ...
```





It is important to consume critical events promptly to prevent loss if the buffer becomes full.