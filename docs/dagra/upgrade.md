# Upgrade Plan

Our Goals:

- Encapsulated processes and information packets.(Nov. 2024)
- The external definition of connections.(Oct. 2024)
- Asynchronous.(Nov. 2024)
- Information packets with unique ownership and defined lifetimes.(Rust features)
- Bounded connections with a finite capacity. (tokio)
- Reserve pressure. (tokio)
- Parser.(Nov. or Dec. 2024)



## Encapsulated processes and information packets

- [ ] Provide a new trait `Node` that defines unique identifiers for nodes, input channels for receiving packets, output channels for sending packets, and an interface to start the workload, replacing the trait `Task` in the old version. 
- [ ] Enable asynchronous operations inside and between processes in trait `Action`.
- [ ] Use `Content` as encapsulation of information packet.

## The external definition of connections

- [ ] Provide asynchronous channels encapsulating the tokio channels and provide a unified interface.

## Asynchronous

- [ ] Provide a struct `Graph`, replacing `Dag` in the old version.
  - [ ] Remove field `rely_graph`.
  - [ ] Automatically create channels and assign corresponding senders and receivers to the nodes when building the graph.
  - [ ] Modify the logic of error-handling: If one node fails, the graph will not stop running, and users can then handle exceptions or errors at a successor node.

## Bounded connections with a finite capacity 

[tokio]: https://tokio.rs/tokio/tutorial	"tokio"

 provides four different channel implementations. 

-  `one-shot` is a channel with a single producer and a single consumer. Only one value can be sent at the same time. 

- `mpsc` is a channel that supports multiple producers and a single consumer. It is different from one-shot in that it supports sending multiple messages. 

- `broadcast` is a channel that supports multiple producers and multiple consumers. Multiple values can be sent. 

- `watch` is a variant of broadcast. The receiver can only see whether the latest value has changed.

We pick `mpsc` and `broadcast` as the channels used in Dagrs because they meet the requirements for connections in FBP: they are bounded, have finite capacity and support reserve pressure.

## Reserve pressure

`tokio` provides congestion handling mechanisms for both `mpsc`and `broadcast` to deal with insufficient channel capacity or the consumer is too slow. 

- `mpsc` will block the sender when the capacity is full. 
- `broadcast` will drop the oldest value, and the slow consumers will get an exception the next time it calls the receive function.

## Parser

- [ ] Support defining custom nodes via macros.
- [ ] Modify the macro `dependencies!` to add dependencies to the already defined nodes (implemented with the `Node` trait) and return with a `Graph` ready to start.