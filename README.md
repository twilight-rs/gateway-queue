[![ci-badge][]][ci] [![license-badge][]][license] [![docs-badge][]][docs] [![rust badge]][rust link]

# Gateway Queue

If your bot's shards are multi-processed, then a good choice is to use the
[Gateway Queue]. The Gateway Queue is a lightweight but powerful application
that you can host to queue shards across all of your processes.

The Gateway Queue is library and language agnostic: it's an HTTP server that you
call whenever a shard needs to reconnect to the gateway.

### How it works

When one of your shards disconnects and needs to perform a full reconnect, then
it needs to start a new session with the gateway. If you have multiple processes
managing shards, then you may not have communication between these processes.
The problems start to happen when multiple shards from these processes try to
reconnect at the same time: all but 1 will get ratelimited.

This is because there's a 5 second ratelimit between new sessions. By sending an
HTTP request to the Gateway Queue, it will ratelimit the requests and "stall"
them, responding with a friendly message once that shard can reconnect:

```json
{"message": "You're free to connect now! :)"}
```

### Example

The API is pretty minimal, probably. It's just an HTTP request:

```rust
reqwest::get("http://gateway-queue").await?;
```

No headers, body, or particular method need to be set. They're all ignored.
The request will get a response once the request has gone through the queue.

### Running it

If you're using Docker, you can clone the repo and run:

```sh
$ docker build . -t dawn-gateway-queue
$ docker run -itd -e HOST=0.0.0.0 -e PORT=5000 dawn-gateway-queue 
```

If you're not, you can compile it via Cargo:

```sh
$ cargo build --release
$ HOST=0.0.0.0 PORT=5000 ./target/release/dawn-gateway-queue
```

`HOST` and `PORT` are the only two environment variables.

[ci-badge]: https://github.com/dawn-rs/gateway-queue/workflows/Test/badge.svg
[ci]: https://github.com/dawn-rs/gateway-queue/actions
[docs]: https://dawn.valley.cafe/chapter_3_services/section_5_gateway_queue.html
[docs-badge]: https://img.shields.io/badge/docs-online-5023dd.svg?style=flat-square
[license-badge]: https://img.shields.io/badge/license-ISC-blue.svg?style=flat-square
[license]: https://opensource.org/licenses/ISC
[LICENSE.md]: https://github.com/dawn-rs/gateway-queue/blob/master/LICENSE.md
[rust badge]: https://img.shields.io/badge/rust-1.39+-(beta)-93450a.svg?style=flat-square
[rust link]: https://blog.rust-lang.org/2019/09/30/Async-await-hits-beta.html
