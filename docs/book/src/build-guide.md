# Build & Verification Guide

## Prerequisites
```bash
rustup target add riscv32imac-unknown-none-elf
cargo install cargo-binutils cargo-bloat
```

## Build
```bash
cargo build --release --target riscv32imac-unknown-none-elf
```

## Static Verification
```bash
cargo fmt --check
cargo clippy --target riscv32imac-unknown-none-elf -- -D warnings
```

## Hardware-in-the-Loop Test (Renode)
Requires [Renode](https://renode.io/) on `PATH`:
```bash
renode-test renode-config/esp32c3.robot
```

## Host-Side Tests & Benchmarks
The `scheduler` crate and the `benchmarks` crate build for the host (not the RISC-V target), so override the workspace's default target explicitly:
```bash
cargo bench -p benchmarks --target x86_64-unknown-linux-gnu
```

## Live Telemetry Dashboard
```bash
pip install -r host/requirements.txt
python host/telemetry_broker.py &
streamlit run host/dashboard.py
```

See the [Fault Injection & Telemetry](fault-injection-and-telemetry.md) chapter for what the dashboard actually shows, and the top-level `README.md` for the full CI gate list.
