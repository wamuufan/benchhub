# BenchHub Installation Guide

This document provides complete instructions for installing dependencies, compiling, and running BenchHub on Ubuntu (22.04 LTS, 24.04 LTS, 26.04 LTS and newer) and Debian-based Linux distributions.

---

## 1. Rust Toolchain Installation

BenchHub is written in Rust. You can install the Rust toolchain on Ubuntu using the official `rustup` package:

```bash
# 1. Update package lists and install rustup
sudo apt update
sudo apt install -y rustup

# 2. Install and set the stable Rust compiler toolchain as default
rustup install stable
rustup default stable
```

*(Alternatively, you can install rustup via the official script: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)*

Verify your installation:
```bash
cargo --version
rustc --version
```

---

## 2. System Dependencies

BenchHub's requirements are categorized into three logical layers:

### A. Build & Compilation Dependencies
Required to compile the C redirection shim (`ssl_shim.c`), font engine bindings, and link the Rust binary:

```bash
sudo apt install -y \
    build-essential \
    pkg-config \
    libfontconfig1-dev \
    libxkbcommon-dev \
    libsqlite3-dev
```

* **`build-essential`**: Provides GCC, G++, Make, and standard libc development headers.
* **`pkg-config`**: Helper tool used by Cargo build scripts to locate system libraries.
* **`libfontconfig1-dev`**: Font discovery library required by Slint and fontdb.
* **`libxkbcommon-dev`**: Keyboard layout handling for Wayland and X11 window backends.
* **`libsqlite3-dev`**: SQLite development headers for local database storage.

### B. Runtime & GUI Dependencies
Required by the Slint graphical user interface to render vector widgets and display clean typography:

```bash
sudo apt install -y \
    fonts-dejavu-core \
    libfontconfig1 \
    libx11-6 \
    libxcursor1 \
    libxrandr2 \
    libxi6 \
    libwayland-client0 \
    libwayland-cursor0 \
    libwayland-egl1
```

* **`fonts-dejavu-core`**: High-quality TrueType font family ensuring crisp text and metric rendering across all UI screens.
* **`libwayland-*` / `libx11-*`**: Native display server and pointer/keyboard event management libraries.

### C. Benchmark Download & Runner Dependencies
Required by BenchHub's execution engine when downloading and extracting benchmark test suites:

```bash
sudo apt install -y \
    curl \
    tar \
    xz-utils \
    unzip
```

---

## ⚡ All-In-One Installation Command

On a clean Ubuntu system, you can install all build, runtime, and benchmark tools with a single command:

```bash
sudo apt update && sudo apt install -y \
    rustup \
    build-essential \
    pkg-config \
    libfontconfig1-dev \
    libxkbcommon-dev \
    libsqlite3-dev \
    fonts-dejavu-core \
    curl \
    tar \
    xz-utils \
    unzip

rustup install stable
rustup default stable
```

---

## 3. Clone and Run

```bash
# 1. Clone the repository
git clone https://github.com/wamuufan/benchhub.git
cd benchhub

# 2. Build and launch the optimized release binary
./run.sh
```

Or run directly with Cargo:
```bash
cargo run --release
```

Or compile a standalone release binary:
```bash
cargo build --release
./target/release/benchhub
```

---

## 4. Optional: Developer Fast Linking (Mold & Clang)

For developers who want 3-5x faster incremental linking times during iterative code changes:

```bash
sudo apt install -y clang mold sccache
```

Then create a `.cargo/config.toml` in your local project root (this file is ignored by `.gitignore`):

```toml
[build]
rustc-wrapper = "sccache"

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

---

## 5. Troubleshooting

### `error: linker 'clang' not found`
* **Cause:** An old, machine-specific `.cargo/config.toml` may be present in your working tree.
* **Solution:** Pull the latest repository changes (`git pull`). BenchHub uses the standard system `gcc` linker by default.

### UI fonts or icons appear as missing boxes
* **Solution:** Install the DejaVu font package:
  ```bash
  sudo apt install -y fonts-dejavu-core
  ```

### Window fails to open under Wayland
* **Solution:** Fall back to the X11 compatibility layer:
  ```bash
  WAYLAND_DISPLAY="" ./run.sh
  ```
