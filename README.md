# err

Sensible Rust error handling.

`err` is a small, dependency-free Rust error type for turning failures into useful diagnostic trails. Wrap errors with context, capture the call site automatically, attach structured data, and inspect every frame later.

## Features

- Zero dependencies
- `#[track_caller]` source locations on created and wrapped errors
- Ordered error frames for readable failure context
- Typed attachments via `attach`
- Human-readable attachments via `attach_printable`
- Debug payloads via `debug`
- Frame and attachment inspection with `frames` and `downcast_ref`
- `Send + Sync` error values

## Usage

Add it to `Cargo.toml`:

```toml
[dependencies]
err = { git = "https://github.com/Vehmloewff/err" }
```

Build context as an operation fails:

```rust
use err::{Err, Result, ResultExt};

fn load_user(id: u64) -> Result<String> {
    fetch_user(id)
        .wrap("loading user")
        .debug("user_id", id)
}

fn fetch_user(_id: u64) -> Result<String> {
    Err(Err::new("user service unavailable"))
}
```

Print the top-level message with `Display`, or print the whole trail with `Debug`:

```rust
match load_user(42) {
    Ok(user) => println!("{user}"),
    Err(error) => eprintln!("{error:?}"),
}
```

Inspect typed attachments:

```rust
let error = Err::new("request failed").attach(42_u16);
assert_eq!(error.downcast_ref::<u16>(), Some(&42));
```

`ResultExt` works with any error implementing `Display + Send + Sync + 'static`:

```rust
use err::{Result, ResultExt};

fn parse_config(input: &str) -> Result<u16> {
    input.parse::<u16>()
        .wrap("parsing port")
        .debug("input", input)
}
```

