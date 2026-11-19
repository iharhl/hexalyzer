# intelhexlib

A Rust library for reading, editing and writing Intel HEX and binary firmware files.
Ships `hexcli`, a command-line tool built on the library.

`intelhexlib` powers [Hexalyzer](https://github.com/iharhl/hexalyzer), a desktop app
for inspecting and editing firmware files.

> [!WARNING]
> `intelhexlib` is still maturing and is not yet production ready. It is tested with
> unit, integration and doc tests, but it has not been fully verified against every
> Intel HEX variant and toolchain, and its API may change between `0.x` releases.
> Bug reports and feedback are very welcome on the
> [issue tracker](https://github.com/iharhl/hexalyzer/issues).

## Features

- **Sparse data support**: data is stored as contiguous chunks in a
  `BTreeMap<usize, Vec<u8>>`. Files with large gaps between memory
  segments don't use extra RAM while lookups stay fast.
- **Reading and writing**: parse Intel HEX or raw binary files, and write either format
  (with a configurable gap-fill byte for binary output).
- **Editing**: update single bytes, slices or ranges, insert and remove ranges, relocate
  to a new start address, and merge files.
- **Searching**: find byte patterns, ASCII strings or regular expressions.
- **Validation**: checksums, record types and addresses are checked while parsing, with
  detailed errors via `IntelHexError`.

## Installation

Run `cargo add intelhexlib`, or add it to the `[dependencies]` section of your `Cargo.toml`:

```toml
[dependencies]
intelhexlib = "0.4"
```

## Usage

```rust
use intelhexlib::IntelHex;
use std::path::PathBuf;

fn main() {
    // Load and parse a file
    let path = PathBuf::from("firmware.hex");
    if let Ok(mut ih) = IntelHex::from_hex(&path) {
        println!("Loaded {} bytes from {:?}", ih.size, ih.filepath);

        // Access data at a specific address
        let address: usize = 0x0800_0000;
        if let Some(byte) = ih.read_byte(address) {
            println!("Byte at {:X}: {:02X}", address, byte);
        }

        // Update data at a specific address
        let new_value = 0x0F;
        match ih.update_byte(address, new_value) {
            Ok(()) => println!("Updated byte at {:X} to: {:02X}", address, new_value),
            Err(err) => println!("Error during update: {err}"),
        }

        // Iterate over all bytes, ignoring address gaps.
        // Since it uses a BTreeMap, keys are always sorted by address.
        // To iterate over contiguous chunks, use ih.iter() instead.
        for (addr, byte) in ih.bytes().take(10) {
            println!("{:X}: {:02X}", addr, byte);
        }
    }
}
```

See the [API documentation](https://docs.rs/intelhexlib) for the full list of operations.

## CLI

`hexcli` can inspect, relocate, convert and merge files. Install it with:

```bash
cargo install intelhexlib --features cli
```

Prebuilt binaries are also attached to each
[Hexalyzer release](https://github.com/iharhl/hexalyzer/releases).

```text
Usage:
  hexcli info <input>
  hexcli relocate <input> <output> [options]
  hexcli convert <input> <output> [options]
  hexcli merge <output> <input1>[:addr] ... <inputN>[:addr] [options]

Options:
  --address <val>    Base address for relocate / convert from BIN to HEX
  --gap-fill <val>   Byte to fill gaps when converting / merging to BIN (default: 0xFF)
  --overwrite        Merge: later inputs replace overlapping data (default: error)

Examples:
  hexcli info firmware.hex
  hexcli relocate firmware.hex firmware_shifted.hex --address 0x1000
  hexcli convert firmware.hex firmware.bin --gap-fill 0x00
  hexcli merge final.hex firmware1.hex firmware2.bin:0xFF00
```

## License

MIT
