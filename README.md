# iso-tp
Async ISO-TP ([ISO 15765-2](https://en.wikipedia.org/wiki/ISO_15765-2)) implementation for embedded devices.

> ISO-TP (Transport Layer), is an international standard for sending data packets over a CAN-Bus. The protocol allows for the transport of messages that exceed the eight byte maximum payload of CAN frames. ISO-TP segments longer messages into multiple frames, adding metadata that allows the interpretation of individual frames and reassembly into a complete message packet by the recipient. It can carry up to 232-1 (4294967295) bytes of payload per message packet starting from the 2016 version. Prior version were limited to a maximum payload size of 4095 bytes.


[![crate](https://img.shields.io/crates/v/iso-tp.svg)](https://crates.io/crates/iso-tp)
[![Rust Documentation](https://img.shields.io/badge/api-rustdoc-blue.svg)](https://docs.rs/iso-tp)
[![CI](https://github.com/matthunz/iso-tp/actions/workflows/rust.yml/badge.svg)](https://github.com/matthunz/iso-tp/actions/workflows/rust.yml)


[Examples](https://github.com/matthunz/iso-tp/tree/main/examples)

## Reading
```rust
use async_hal::io::AsyncRead;
use iso_tp::Socket;

let mut socket = Socket::new(tx, rx);
let mut reader = socket.reader();

let mut buf = [0; 64];
reader.read_to_end(&mut buf).await?;

dbg!(&buf);
```

## Writing
```rust
use async_hal::io::AsyncWrite;
use iso_tp::Socket;

let mut socket = Socket::new(tx, rx);
let mut writer = socket.writer();

writer.write_all(b"Hello World!").await?;
```
