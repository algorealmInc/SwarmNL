**SwarmNL: A library to build custom networking layers for decentralized applications.**

Adedeji Adebayo (git: @thewoodfish), Sacha Lansky (git: @sacha-l)

\
\


**ABSTRACT**

The rise of decentralized and distributed applications has ushered in a new era of innovation, but building such systems remains a complex and resource-intensive challenge. From ensuring reliability to balancing trade-offs, developers face numerous hurdles when implementing robust distributed systems. To address this, we present **SwarmNL**, a platform designed to abstract the intricacies of distributed systems. SwarmNL simplifies the development process by providing a dependable networking layer for distributed communication, enabling developers to focus on application-specific functionality without getting bogged down by underlying system complexities. This paper introduces the core principles, architecture, and benefits of SwarmNL, paving the way for streamlined and scalable distributed applications.

\


**INTRODUCTION**

The journey to creating a decentralized and distributed self-sovereign database revealed significant challenges. We found ourselves repeatedly building custom networking solutions and wrestling with trade-offs tightly intertwined with the application layer. Developing on the libp2p protocol \[1], while powerful, exposed its own set of complexities. Application developers not only need to understand the intricacies of the protocol but also expend considerable effort constructing a functional network layer, navigating trade-offs like latency, scalability, and availability. Only after overcoming these hurdles could they turn their attention back to building the application itself.

The networking layer, however, should be a means to an end—a black box that is easily configurable to meet application requirements, allowing developers to focus on innovation.

This realization led us to create **SwarmNL**. Built on top of the libp2p protocol, SwarmNL leverages the decentralized principles and capabilities of libp2p to provide a comprehensive framework for building scalable decentralized and distributed networks. Our library simplifies the complexities of network design by addressing critical challenges like redundancy, replication, fault-tolerance, and other considerations tied to latency, availability, and scalability.

SwarmNL equips application developers with intuitive, configurable interfaces, enabling them to integrate robust networking capabilities seamlessly. This allows them to devote their energy to crafting application logic, knowing that the networking layer is already optimized, reliable, and scalable.

**BACKGROUND/RELATED WORK**

Distributed systems and decentralized architectures have become integral to modern applications, particularly in areas like peer-to-peer networks, blockchain technologies, and self-sovereign databases. These systems promise scalability, fault tolerance, and resilience by eliminating single points of failure. However, achieving these benefits often comes with significant challenges, including complex networking protocols, trade-offs between consistency and availability, and the need for robust fault-tolerance mechanisms.

The **libp2p protocol** is a notable solution in this space, serving as a modular network stack designed for peer-to-peer applications. Libp2p provides building blocks for establishing connections, managing peers, and facilitating communication across distributed nodes. Its flexibility allows developers to tailor solutions for various use cases, from file sharing to blockchain infrastructure. However, this flexibility also introduces complexity. Developers must navigate numerous trade-offs, such as optimizing for latency versus throughput, managing redundancy, and balancing security with performance.

Several projects have attempted to streamline these complexities. For instance:

1. **IPFS (InterPlanetary File System):** Built on libp2p, IPFS focuses on content-addressable, peer-to-peer file sharing. While it abstracts some networking details, its scope is limited to data storage and retrieval rather than providing a general-purpose networking framework.

2. **BitTorrent Protocols:** These protocols are widely used for decentralized file sharing but lack the configurability and scalability required for more complex distributed applications.

3. **WebRTC:** Primarily designed for real-time communication, WebRTC provides peer-to-peer connectivity but requires extensive customization for broader distributed system use cases.

Despite these advancements, developers still face significant hurdles when implementing a decentralized network layer. Many existing solutions are tightly coupled to specific applications or require in-depth knowledge of networking protocols, forcing developers to reinvent the wheel for each project.

**SwarmNL** builds on these lessons and leverages the power of libp2p to offer a streamlined, developer-friendly solution. By abstracting the complexities of distributed networking, SwarmNL bridges the gap between a powerful decentralized protocol and a scalable, configurable framework. It addresses common pain points such as redundancy, replication, and fault-tolerance while empowering developers to focus on application-specific functionality.

\


TECHNOLOGY

SwarmNL is built on libp2p and exposes simple configurable and operational interfaces to the application to configure and interact with the network.

![](https://lh7-rt.googleusercontent.com/docsz/AD_4nXe1KPHQ5dmPiDG7BCCJZvocXgT7YhXmL-eaIwwo1Kh7V5dM2nhD8SwgYhx6X15BEsI5NMTTkoknGSfBmdkjk6GjUe5ejrQMtM2NqJu8XG9d5j1wnJ3jq2XfLsZ8eOlGq052exA8?key=zqthnCajXKJ-fWwxhjNjlPqb)

The application layer is exposed to various operations that it requires to participate in any network activity. These include:

1. Node identify set up

2. Connecting to peers

3. Communicating to peers

4. Replication etc.

The application calls a function that takes in a data structure to direct the network layer to act. The data structure \`AppData\` is an enum containing commands that the network layer carries out. 

\
\
\
\
\
\
\
\
\
\
\
\
\
\


\[1] Libp2p protocol specification https\://github.com/libp2p/specs
