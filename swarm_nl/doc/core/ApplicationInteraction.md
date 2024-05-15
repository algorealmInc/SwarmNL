# Application Interaction

The core library provides very simple interfaces to communicate with the network layer to trigger and drive network behaviour or to make enquiries about internal network state.
This is achieved by constructing a request with the [`AppData`] struct and passing it to the network. An [`AppResponse`] structure is returned, containing the network's response to the applications request.<br>
There are two ways of querying the network layer with each having its own peculiarities and use-cases:
- Using the [`Core::query_network()`] method: This method aims to complete its operations atomically and blocks until the network returns a reponse or if the request times out. It is useful when the response from the network is important for the application logic to continue.
```rust
    // Default config
    let config = BootstrapConfig::default();
    // Default handler
    let handler = DefaultHandler;

    // Create a default network core builder
    let node = CoreBuilder::with_config(config, handler);

    // Join a network (subscribe to a topic)
    let gossip_request = AppData::GossipsubJoinNetwork(MY_NETWORK.to_string());

    // Blocks until response is returned from the network layer
    if let Ok(_) = node.query_network(gossip_request).await {
        println!("Subscription successfull");
    }
```

- Using the [`Core::send_to_network()`] and [`Core::recv_from_network()`] method: This method does not block and is split into two parts - sending and recieving. When the request is recieved by the network layer through the [`Core::send_to_network()`] method, a [`StreamId`] is immediately returned and the request is handled. When there is a response (or a timeout), it is stored internally in a response buffer until it is returned by explicitly polling the network through the [`Core::recv_from_network()`] method which takes in the [`StreamId`] returned earlier. The [`StreamId`] helps to track the requests and their corresponding responses internally in the network layer. 
```rust
    // Default config
    let config = BootstrapConfig::default();
    // Default handler
    let handler = DefaultHandler;

    // Create a default network core builder
    let node = CoreBuilder::with_config(config, handler);

    // Join a network (subscribe to a topic)
    let gossip_request = AppData::GossipsubJoinNetwork(MY_NETWORK.to_string());

    // Send request to the application layer
    let stream_id = node.send_to_network(gossip_request).await.unwrap();

    // ...
    // Run other application logic
    //...

    // Explicitly retrieve the response of our request
    if let Ok(result) = node.recv_from_network(stream_id).await {
        println!("Subscription successfull");
        assert_eq!(AppResponse::GossipsubJoinSuccess, result);
    }
```
Note: The internal buffer is limited in capacity and pending responses should be removed as soon as possibe. A full buffer will prevent the network from recieving more requests.