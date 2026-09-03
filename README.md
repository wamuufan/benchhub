# BenchHub

BenchHub is an automation and UI tool for Linux that allows you to easily download, run, and analyze popular hardware benchmarking tools.

## Legal & Licensing

The BenchHub project is licensed under the **BSD 2-Clause License**. You are free to use, modify, and distribute the source code under these terms. (See the `LICENSE` file for details).

**HOWEVER, you must be aware of the following legal conditions and warnings regarding the use and distribution of this project:**

### 1. Open Source Dependencies (GPLv3)
BenchHub's graphical user interface (UI) is built using the open-source **Slint** framework. When used in open-source projects, Slint is subject to the **GPLv3** license.
* Therefore, if you compile BenchHub from source and distribute it as an executable binary, the entire application (due to the Slint integration) **will be subject to the terms of the GPLv3 license.**
* While your own source code remains under the BSD 2-Clause license, the distributed binary must comply with GPLv3 rules (such as providing source code to users). (Unless you own a commercial license for Slint).

### 2. Third-Party Benchmarking Tools & EULAs
BenchHub automates the downloading and execution of external, proprietary benchmarking tools such as *Geekbench, Unigine (Superposition, Heaven), Blender, and Y-Cruncher*.
* **All of these software packages are governed by their own End User License Agreements (EULA).** BenchHub never automatically accepts these EULAs on your behalf. When installing a benchmark via the UI, it is entirely your responsibility to read and accept these terms.
* For instance, running the free (Basic) versions of Unigine and Geekbench in a commercial or corporate environment in an **automated** manner may violate their respective EULAs. BenchHub is merely a "trigger" tool; the creators of BenchHub cannot be held liable for any commercial or personal EULA violations committed by the end-user.

### 3. Trademarks Disclaimer
The names of third-party software, companies, and products mentioned in this software and its source code (e.g., Geekbench, Unigine, Blender, AMD, NVIDIA, etc.) are **registered trademarks of their respective owners.**
* BenchHub is an independent tool. We have **no** official partnership, affiliation, or sponsorship with these companies.
* The use of these names falls under "Nominative Fair Use" principles, strictly for the purpose of identifying the benchmarking tools.

## Installation and Usage

```bash
cargo build --release
./target/release/benchhub
```
