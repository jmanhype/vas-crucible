# VAS-Crucible

Rust sidecar that manages sandboxed code execution for V.A.O.S. agents. Receives gRPC requests, validates JWT intent tokens, creates Docker containers with resource limits, and optionally enforces syscall policy via eBPF.

## Status

v0.1.0. The gRPC server, JWT verifier, and Docker sandbox manager compile and have integration test stubs. The eBPF enforcer is behind a feature flag and requires a Linux kernel with BTF support. Tests use an in-process backend rather than real Docker containers.

## Repository layout

```
38 files total

src/
  main.rs                     -- CLI entry point, starts gRPC listener
  lib.rs                      -- re-exports 5 modules
  sidecar.rs                  -- sidecar lifecycle logic
  jwt/
    mod.rs
    claims.rs                 -- JWT claim structure
    verifier.rs               -- HS256 verification, 60s TTL enforcement
  sandbox/
    mod.rs
    docker.rs                 -- Docker container creation via bollard
    limits.rs                 -- CPU, memory, network policy
    pty.rs                    -- PTY allocation for container sessions
  enforcer/
    mod.rs
    ebpf_loader.rs            -- eBPF program loading (behind "ebpf" feature)
    syscalls.rs               -- syscall allow/deny policy
  grpc/
    mod.rs
    server.rs                 -- SandboxControl gRPC service impl
    client.rs                 -- gRPC client for testing/integration

ebpf/
  Cargo.toml
  src/
    main.rs                   -- eBPF userspace loader
    bpf/syscall_hook.c        -- BPF C program for syscall interception
  headers/vmlinux.h           -- kernel type definitions for BPF

protobuf/vas.proto            -- gRPC service definition (4 RPCs)
tests/
  integration.rs
  integration/
    grpc_tests.rs
    jwt_tests.rs
    sandbox_tests.rs
  common/mod.rs               -- shared test helpers
```

## gRPC API

Defined in `protobuf/vas.proto`. Service: `SandboxControl`.

| RPC | Request | Response | Purpose |
|-----|---------|----------|---------|
| CreateSandbox | agent_id, jwt, intent_hash, resource_limits | sandbox_id, pty_path, created_at | Start an isolated container |
| ExecuteCode | sandbox_id, jwt, code, language | exit_code, stdout, stderr, duration_ms | Run code in an existing sandbox |
| TerminateSandbox | sandbox_id, jwt | (empty) | Destroy a sandbox |
| Heartbeat | sandbox_id, jwt | alive, last_seen | Check sandbox liveness |

Every RPC requires a JWT. The `intent_hash` in the token must match the requested operation.

## Modules

| Module | Files | Purpose |
|--------|-------|---------|
| jwt | 3 | HS256 token verification with 60-second TTL window |
| sandbox | 4 | Docker container lifecycle via bollard, resource limits (CPU cores, memory MB, network toggle), PTY allocation |
| grpc | 3 | tonic-based gRPC server and client |
| enforcer | 3 | Optional eBPF syscall policy (feature-gated) |
| sidecar | 1 | Top-level sidecar coordination |

## Dependencies

27 crate dependencies. Key ones:

| Crate | Purpose |
|-------|---------|
| tonic 0.12 | gRPC server/client |
| prost 0.13 | Protobuf code generation |
| bollard 0.17 | Docker Engine API client |
| jsonwebtoken 9.3 | JWT encoding/decoding |
| tokio 1.39 | Async runtime |
| aya 0.13 | eBPF loading (optional, behind `ebpf` feature) |
| tracing 0.1 | Structured logging |

## Security model

- JWT verification is mandatory on every RPC call
- Tokens have a hard 60-second TTL
- Each token includes an `intent_hash` that must match the operation
- Sandbox containers get explicit CPU, memory, and network limits
- The optional eBPF layer can kill processes whose JWT state becomes invalid

## Requirements

- Rust 1.80+
- Docker Engine (accessible from the sidecar process)
- For eBPF: clang, Linux kernel with BTF support

## Build

```bash
cargo build                    # without eBPF
cargo build --features ebpf    # with eBPF enforcer
```

## Run

```bash
cargo run -- --listen 0.0.0.0:50051 --hs256-secret dev-secret
```

## Tests

```bash
cargo test
```

Integration tests use an in-process backend. They do not require a running Docker daemon.

## Design decisions

**JWT intent hashing**: Each request must carry a token whose `intent_hash` matches the specific operation. This prevents token reuse across different actions, even within the same 60-second window.

**eBPF behind a feature flag**: The eBPF enforcer depends on kernel-specific headers (`vmlinux.h`) and a Linux-only toolchain. Keeping it optional means the core sandbox works on macOS/Docker Desktop for development.

**In-process test backend**: Real Docker tests are slow and environment-dependent. The in-process backend gives deterministic test results in CI without a Docker daemon.

**Workspace with separate eBPF crate**: The BPF C code and its Rust loader live in `ebpf/` as a separate workspace member, isolating the kernel-space build from the main application.

## Limitations

- No TLS on the gRPC endpoint (expects a service mesh or reverse proxy)
- eBPF enforcer only works on Linux with BTF-enabled kernels
- No container image caching or pooling -- each sandbox creates a fresh container
- PTY support (`sandbox/pty.rs`) is Unix-only (uses `nix` crate)
- No rate limiting or quota management

## License

MIT.
