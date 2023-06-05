# iso-tp
Async ISO-TP (ISO 15765-2) implementation for embedded devices

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
