# BenchHub

BenchHub is a comprehensive Linux hardware benchmarking, stress testing, real-time telemetry, and result analysis suite designed for performance enthusiasts, hardware reviewers, and system engineers.

## Architecture Overview

BenchHub is built on a clean dual-binary architecture communicating over a local Unix domain socket:

```
┌────────────────────────────────────────────────────────┐
│                   benchhub-desktop                     │
│         Tauri v2 + Leptos 0.8 (CSR WASM) UI            │
│  - Bilingual Localization (en / tr)                    │
│  - Linux Desktop Theme Sync (KDE Breeze & Adwaita)    │
└──────────────────────────┬─────────────────────────────┘
                           │ Unix Domain Socket IPC
┌──────────────────────────▼─────────────────────────────┐
│                       benchhubd                        │
│            Background Orchestrator Daemon              │
│  - Benchmark Installation & Execution Lifecycle        │
│  - Process-Group Management & Thermal Conditioning     │
│  - Real-Time Telemetry (NVML, hwmon, RAPL, DRM)        │
│  - SQLite Run History & CSV/PNG Report Exports         │
└────────────────────────────────────────────────────────┘
```

- **`benchhubd` (`crates/benchhub-daemon` + `crates/benchhub-core`)**:
  A lightweight, headless background orchestrator daemon. It manages benchmark profile definitions, tool downloads/installations, process-group lifecycles, thermal pre-conditioning routines, real-time multi-source telemetry sampling (`NVML`, `hwmon`, `RAPL`, `DRM`), SQLite benchmark run history, and report generation (CSV / PNG) over an authenticated local Unix domain socket.

- **`benchhub-desktop` (`crates/benchhub-gui` + `src-tauri`)**:
  A thin, responsive desktop application frontend powered by Tauri v2 and Leptos 0.8 (Client-Side Rendering via WebAssembly). It features full bilingual localization (`en` / `tr`), dynamic Linux desktop theme synchronization (KDE Plasma Breeze and GNOME/GTK Libadwaita palettes), real-time telemetry charts, queue management, and comparative history analysis.

## Legal & Licensing

BenchHub is licensed under the **BSD 2-Clause License**. You are free to use, modify, and distribute the source code under these terms. See the [LICENSE](LICENSE) file for the complete text.

### Third-Party Benchmarking Tools & EULAs

BenchHub automates the discovery, downloading, and execution of external benchmarking tools (such as Geekbench, Unigine Superposition/Heaven, Blender Benchmark, Y-Cruncher, 7-Zip, FurMark, and others).

- **All third-party tools are governed by their respective End User License Agreements (EULA) and terms of service.** BenchHub never automatically accepts EULAs on your behalf; explicit user confirmation is required in the user interface prior to download or execution.
- Automated or commercial execution of certain third-party benchmarks (e.g., free editions of commercial suites) may be subject to licensing restrictions. BenchHub is an execution orchestrator; users and organizations are solely responsible for ensuring compliance with all vendor licenses.

### Trademarks Disclaimer

All product names, logos, brands, and registered trademarks mentioned within BenchHub and its documentation (e.g., Geekbench, Unigine, Blender, AMD, Intel, NVIDIA) are property of their respective owners. Their mention in BenchHub is solely for identification and compatibility purposes and constitutes **Nominative Fair Use**. BenchHub is an independent open-source project and is not affiliated with, endorsed by, or sponsored by any of these trademark holders.

## Quick Start

For complete system prerequisites, prebuilt release installation, and Flatpak/Snap packaging instructions, please refer to the **[Installation Guide](INSTALLATION.md)**.

### 1. Build and Package Release Archive

```bash
./scripts/build.sh
```

This builds `benchhubd`, the Leptos WASM frontend, and `benchhub-desktop`, and produces `benchhub-linux-x86_64.tar.gz` (plus `.sha256`).

### 2. Install Locally

```bash
mkdir -p /tmp/benchhub-install
tar -xzf benchhub-linux-x86_64.tar.gz -C /tmp/benchhub-install
cd /tmp/benchhub-install/benchhub-linux-x86_64
./install.sh
```

Or launch directly from the workspace during development:

```bash
./scripts/run.sh
```
