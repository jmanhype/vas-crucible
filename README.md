# VAS-Crucible

VAS-Crucible is a sidecar service that provides authenticated sandbox lifecycle management for agent execution. It combines JWT intent validation, Docker-based isolation, gRPC control, and optional eBPF policy enforcement.

## Architecture

```text
           +------------------+
           |   VAS-Swarm      |
           |  control plane   |
           +---------+--------+
                     |
                gRPC + JWT
                     |
         +-----------v------------+
         |      VAS-Crucible      |
         |  sidecar / gRPC API    |
         +-----+-----------+------+
               |           |
      validates JWT     manages PTY
               |           |
         +-----v-----+ +---v------------------+
         | Sandbox   | | Docker containers    |
         | registry  | | CPU / memory limits  |
         +-----+-----+ +---+------------------+
               |           |
               +-----+-----+
                     |
               optional eBPF
                     |
         +-----------v------------+
         | syscall policy / kill  |
         | switch / event stream  |
         +------------------------+
```

## Requirements

- `rustc` 1.80 or newer
- `clang` and a recent Linux kernel with BTF support for the eBPF component
- Docker Engine reachable from the sidecar

## Build

```bash
cargo build
```

To enable the eBPF loader integration:

```bash
cargo build --features ebpf
```

## Run

```bash
cargo run -- --listen 0.0.0.0:50051 --hs256-secret dev-secret
```

## Usage

The service exposes the `SandboxControl` gRPC API defined in [`protobuf/vas.proto`](./protobuf/vas.proto). Each request must include a JWT whose `intent_hash` matches the operation being performed.

## Security Notes

- JWT verification is always enforced.
- Tokens are limited to a hard 60 second TTL window.
- Sandbox creation applies CPU, memory, and network policy.
- The optional eBPF layer can terminate processes associated with invalid JWT state.

## Development

```bash
cargo fmt
cargo test
```

The Docker-backed sandbox implementation is production-oriented, but the tests use an in-process backend to remain deterministic in constrained environments.

