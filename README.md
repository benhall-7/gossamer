# Gossamer

A lightweight, experimental HTTP web server.

## Design philosophy

1. Low-level, high-clarity. Route handlers just take requests and return responses. Function args are not variadic, and don't map unpredictably to different parts of the HTTP request.
2. Extensability. OpenAPI should be easy to integrate, but entirely optional.
3. Dry. When possible, avoid repetition and stick to one source of truth.

Utilizes hyper as a base.
