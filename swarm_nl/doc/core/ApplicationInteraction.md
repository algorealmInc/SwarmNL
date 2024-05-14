# Application Interaction

The core library provides a request/response protocol that handles sending and receiving data to and from your application. All requests are handled by the [`AppData`] data structure and all responses are handled by the [`AppResponse`] data structure.

## Request / response protocol

When request comes in it gets decoded into a `Vec` of strings, then it’s sent to the function configured to answer requests.

TODO
