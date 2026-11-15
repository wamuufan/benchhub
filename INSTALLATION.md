# BenchHub Installation Guide

This guide covers installing BenchHub from the prebuilt Linux release archive (`benchhub-linux-x86_64.tar.gz`), building from source, and creating Flatpak or Snap distribution packages.

---

## 1. Runtime Dependencies

BenchHub's desktop shell (`benchhub-desktop`) uses Tauri v2 (WebKit2GTK + Ayatana AppIndicator), and `benchhubd` uses standard archiving/download utilities for benchmark suites:

- `libwebkit2gtk-4_1-0` (Ubuntu/Debian: `libwebkit2gtk-4.1-0`)
- `libayatana-appindicator3-1`
- `curl`, `tar`, `xz`, `unzip`

**openSUSE Tumbleweed:**
```bash
sudo zypper install -y libwebkit2gtk-4_1-0 libayatana-appindicator3-1 curl tar xz unzip
```

**Ubuntu / Debian:**
```bash
sudo apt update && sudo apt install -y \
    libwebkit2gtk-4.1-0 \
    libayatana-appindicator3-1 \
    curl \
    tar \
    xz-utils \
    unzip
```

---

## A. Prebuilt Release Package (`benchhub-linux-x86_64.tar.gz`)

1. Download `benchhub-linux-x86_64.tar.gz` (and optionally `benchhub-linux-x86_64.tar.gz.sha256`) from [GitHub Releases](https://github.com/wamuufan/benchhub/releases).
2. Extract the archive and run the installer:

```bash
tar -xzf benchhub-linux-x86_64.tar.gz
cd benchhub-linux-x86_64
./install.sh
```

> On openSUSE Tumbleweed, `./install.sh` automatically checks and installs missing runtime dependencies via `zypper`.
> On other Linux distributions, install the runtime dependencies from Section 1 first, then run:
> ```bash
> ./install.sh --skip-deps
> ```

---

## B. Build from Source

### 1. Build Dependencies

**openSUSE Tumbleweed:**
```bash
sudo zypper install -y \
    gcc gcc-c++ make pkg-config \
    sqlite3-devel libopenssl-devel \
    gtk3-devel webkit2gtk3-soup2-devel libayatana-appindicator3-devel librsvg-devel \
    fontconfig-devel libxkbcommon-devel libX11-devel \
    curl tar xz unzip
```

**Ubuntu / Debian:**
```bash
sudo apt update && sudo apt install -y \
    build-essential pkg-config \
    libsqlite3-dev libssl-dev \
    libglib2.0-dev libgtk-3-dev libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev \
    libayatana-appindicator3-dev librsvg2-dev \
    libfontconfig1-dev libxkbcommon-dev libx11-dev \
    curl tar xz-utils unzip
```

**Rust Toolchain & Frontend Tools:**
```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

### 2. Compilation

```bash
git clone https://github.com/wamuufan/benchhub.git
cd benchhub
chmod +x scripts/build.sh
./scripts/build.sh
```

This builds `benchhubd`, the Leptos WASM frontend, and `benchhub-desktop`, and packages `benchhub-linux-x86_64.tar.gz` and `benchhub-linux-x86_64.tar.gz.sha256` in the repository root.

### 3. Extract and Install the Built Package

```bash
mkdir -p /tmp/benchhub-install
tar -xzf benchhub-linux-x86_64.tar.gz -C /tmp/benchhub-install
cd /tmp/benchhub-install/benchhub-linux-x86_64
./install.sh
```

---

## C. Building Distribution Packages (Flatpak & Snap)

BenchHub also provides packaging scripts for Flatpak and Snap:

- **Flatpak (`benchhub-linux-x86_64.flatpak`):**
  ```bash
  ./scripts/build-flatpak.sh
  ```
- **Snap (`benchhub-linux-x86_64.snap` via LXD):**
  ```bash
  ./scripts/build-snap.sh
  ```

---

## Launch & Uninstall

- **Launch:** Open **BenchHub** from your desktop application menu, or run `benchhub` in your terminal (`~/.local/bin/benchhub`).
- **Uninstall:**
  ```bash
  ~/.local/bin/Benchhub/uninstall.sh
  ```
  *(User benchmark history and downloaded runners in `~/.local/share/benchhub` are preserved unless removed manually.)*
