# Hezi: A Command Line Archive Utility

=====================================================

Hezi is a command line archive utility that allows you to create, extract, and list the contents of archives. The name "hezi" comes from mandarin "盒子" (hézi), which means "box" in English, representing the idea of containing and managing files.

### Usage

```
hezi [OPTIONS] <COMMAND>
```

### Commands

- `list`: List the contents of an archive.
- `create`: Create a new archive.
- `extract`: Extract the contents of an archive.
- `help`: Print this help message or the help for a specific subcommand.

### Options

```
--color <COLOR>  Color [default: auto] [possible values: always, auto, never]
-v, --verbose    Verbosity level
--json           Json output
-h, --help       Print help
-V, --version   Print version
```

### Subcommands

#### List

```
hezi list [OPTIONS] <PATH>
```

- `<PATH>`: The path to the archive to list.
- Options:
  - `--color <COLOR>`: Color [default: auto] [possible values: always, auto, never]
  - `-l, --long`: Detailed output
  - `-p, --password <PASSWORD>`: Password of the archive
  - `-v, --verbose`: Verbosity level
  - `--json`: Json output
  - `-h, --help`: Print help

#### Create

```
hezi create [OPTIONS] <ARCHIVE_ PATH> [FILE]...
```

- `<ARCHIVE_PATH>`: The path of the archive to create.
- `[FILE]...`: Files to add to the archive.
- Options:
  - `--color <COLOR>`: Color [default: auto] [possible values: always, auto, never]
  - `-d, --directory <DIRECTORY>`: Directory to use as the root of the archive
  - `-l, --level <LEVEL>`: Compression level
  - `-v, --verbose`: Verbosity level
  - `--json`: Json output
  - `-o, --overwrite`: Force overwrite
  - `-c, --compression <COMPRESSION>`: Compression algorithm [possible values: gzip, bzip2, lzma, zstd, aes, deflate, none]
  - `-p, --password <PASSWORD>`: Password
  - `-h, --help`: Print help

#### Extract

```
hezi extract [OPTIONS] <PATH>
```

- `<PATH>`: The path of the archive to extract.
- Options:
  - `--color <COLOR>`: Color [default: auto] [possible values: always, auto, never]
  - `-o <OUT>`: The path to write to
  - `-f, --force`: Overwrite existing files
  - `-v, --verbose`: Verbosity level
  - `--json`: Json output
  - `-p, --password <PASSWORD>`: A password to use
  - `-h, --help`: Print help

## Development

### Prerequisites

- Rust

#### Zstd

Some systems may require the installation of the `zstd` library. For example, on Ubuntu, you can install it with the following command:

```sh
sudo apt install libzstd-dev
```

### Build

```sh
cargo build
```
