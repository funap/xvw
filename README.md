<div align="center">

# ⚡ xvw

### **A fast, GPU-accelerated binary & hex editor written in Rust.**

Built on **[GPUI](https://gpui.rs/)** (the high-performance GPU UI framework powering [Zed](https://zed.dev/)).  
Engineered to be snappy, intuitive, and versatile for reverse engineering, firmware inspection, and binary format debugging.

[![GitHub License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)
[![GPUI Powered](https://img.shields.io/badge/powered%20by-GPUI-blueviolet.svg)](https://gpui.rs/)
[![Repository](https://img.shields.io/badge/github-funap%2Fxvw-informational.svg)](https://github.com/funap/xvw)

<br />

<p align="center">
  <img src="docs/images/main.png" alt="xvw Main Window" width="900" style="border-radius: 8px; box-shadow: 0 8px 30px rgba(0,0,0,0.3);" />
</p>

</div>

---

> [!WARNING]
> **Alpha Software Notice & Disclaimer**  
> `xvw` is currently in early alpha stage and has not been fully evaluated or hardened. Please **make reliable backups** of your files before editing. Use at your own risk.  
> We actively welcome feedback, bug reports, and contributions! Feel free to [open an issue](https://github.com/funap/xvw/issues) or submit a pull request.

---

## ✨ Features at a Glance

- ⚡ **Instant Multi-GB File Loading**  
  Memory-mapped I/O (`memmap2`) allows gigabyte-sized files and disk dumps to open immediately with negligible RAM usage and zero lag.
- 🧬 **Interactive Kaitai Struct Parsing**  
  Load `.ksy` format definitions at runtime to dissect complex binary formats (ZIP, ELF, Mach-O, PNG, etc.) with structured tree navigation, inline color highlights, and parsed values.
- ✂️ **Custom Logical Line Breaks**  
  Hit `Enter` anywhere to break lines at natural packet or record boundaries instead of being locked into a rigid 16-byte grid. Press `Cmd+J` / `Ctrl+J` to join lines back.
- 🔍 **Real-Time Data Inspector**  
  Inspect any byte or selection decoded simultaneously into `i8`–`i64`, `u8`–`u64` (Little/Big Endian), `f32`/`f64`, Unix timestamps, binary bits, and character representations.
- 📊 **2D Visual Map & Entropy View**  
  Render byte values as a 2D bitmap with grayscale, byte-category, and rainbow color modes to visually identify code segments, compressed sections, and encrypted payloads.
- ⚖️ **Synchronized Side-by-Side Diff**  
  Compare two binary files side by side with synchronized scrolling, difference counters, and clear delta highlights.
- 🌐 **40+ Text Encodings**  
  Decode and search strings across UTF-8, UTF-16, Shift-JIS, EUC-JP, GB18030, Big5, ISO-8859 variants, Windows code pages, and legacy character sets.
- 📋 **Rich "Copy As" Exports**  
  Quickly export selections as C/C++ arrays, Rust arrays, JSON arrays, Base64 strings, Hex streams, printable text, or formatted Hex dumps.
- 🎯 **Go to Address & Range Selection**  
  Jump to any offset (hex, decimal, relative, or percentage) or select address ranges (e.g. `0x20..0x1ff`).
- ⌨️ **Vim-Inspired Keybindings**  
  Navigate effortlessly with `h`/`j`/`k`/`l`, expand selections with `Shift` modifiers, and search with `/` across hex patterns, text, and regex.

---

## 📸 Feature Showcase

### 🧬 Kaitai Struct Binary Analysis
Load format definitions on the fly to inspect fields, offsets, and nested data structures directly mapped over raw bytes.

<p align="center">
  <img src="docs/images/structure.png" alt="Kaitai Struct Parsing" width="850" style="border-radius: 6px;" />
</p>

### 📊 2D Visual Map / Entropy Inspection
Spot byte patterns, compression boundaries, and encryption blocks with 2D heatmaps and custom palette modes.

<p align="center">
  <img src="docs/images/visual_map.png" alt="2D Visual Map" width="850" style="border-radius: 6px;" />
</p>

### ⚖️ Side-by-Side Binary Comparison (Diff)
Verify firmware patches, inspect file mutations, and track binary deltas with synchronized dual-pane diffing.

<p align="center">
  <img src="docs/images/diff.png" alt="Binary Diff View" width="850" style="border-radius: 6px;" />
</p>

### 🔍 Real-Time Data Inspector & Decoders
Decode values on the fly across multiple numeric and text representations with configurable endianness.

<p align="center">
  <img src="docs/images/inspector.png" alt="Data Inspector" width="850" style="border-radius: 6px;" />
</p>

---

## 🚀 Getting Started

### Installation

#### Pre-built Binaries (Recommended)

Download the latest release for your operating system from the [Releases](https://github.com/funap/xvw/releases) page:

- **macOS** (`.dmg`):
  1. Download `xvw-<version>-aarch64-apple-darwin.dmg`.
  2. Open the disk image and drag **`xvw.app`** into your **Applications** folder.
  
  > [!TIP]
  > **macOS: "“xvw” is damaged and can’t be opened"**  
  > If macOS Gatekeeper blocks the app with a "damaged" warning, it is because open-source releases are not notarized through Apple's paid developer program. The binary is completely safe and intact.  
  > To allow `xvw` to run, open **Terminal** and run:
  > ```bash
  > xattr -cr /Applications/xvw.app
  > ```
  > *(Alternatively, **Control-click** (right-click) `xvw.app` in Finder, select **Open**, and click **Open** in the confirmation dialog).*

- **Linux** (`.deb` / `.rpm`):
  - **Debian / Ubuntu**: `sudo dpkg -i xvw_<version>_linux_amd64.deb`
  - **Fedora / RHEL**: `sudo rpm -i xvw-<version>-1.linux.x86_64.rpm`
- **Windows** (`.zip`):
  - Download and extract `xvw-<version>-x86_64-pc-windows-msvc.zip` and run `xvw.exe`.

#### Build from Source

##### Prerequisites

- **Rust** (2024 edition / latest stable)  
  Install via [rustup.rs](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

##### Build and Run

```bash
# Clone the repository
git clone https://github.com/funap/xvw.git
cd xvw

# Build and run
cargo run --release
```

### Command-Line Usage

`xvw` comes with a powerful CLI interface allowing you to launch straight into your target analysis workflow:

```bash
# Open a specific binary file
xvw path/to/binary_file.bin

# Open a project folder / workspace
xvw --folder path/to/folder

# Open a file and automatically apply a Kaitai Struct definition
xvw firmware.bin --ksy specs/firmware.ksy

# Compare two binary files side-by-side
xvw --diff original.bin patched.bin

# Open a file directly with a specific sidebar panel
# (Options: files, strings, structure, inspector, map, checksum, bookmarks)
xvw binary.bin --panel map
```

---

## ⌨️ Keybindings Cheat Sheet

### Navigation & Editing

| Action | macOS | Linux / Windows |
|---|---|---|
| **Move Cursor** | `h` / `j` / `k` / `l` or Arrows | `h` / `j` / `k` / `l` or Arrows |
| **Expand Selection** | `Shift + h/j/k/l` | `Shift + h/j/k/l` |
| **Jump to Start of File** | `Cmd + Home` / `Home` | `Ctrl + Home` / `Home` |
| **Jump to End of File** | `Cmd + End` / `End` | `Ctrl + End` / `End` |
| **Go to Address / Range** | `Cmd + L` / `Ctrl + G` | `Ctrl + L` / `Ctrl + G` |
| **Insert Custom Break** | `Enter` | `Enter` |
| **Join Lines** | `Cmd + J` | `Ctrl + J` |
| **Reset Custom Breaks** | `Cmd + Shift + Backspace` | `Ctrl + Shift + Backspace` |
| **Toggle Insert / Overwrite** | `Insert` | `Insert` |

### Search & Workspace

| Action | macOS | Linux / Windows |
|---|---|---|
| **Search (Inline)** | `/` or `Cmd + F` | `/` or `Ctrl + F` |
| **Search Panel (Scan All)** | `Cmd + Shift + F` | `Ctrl + Shift + F` |
| **Next / Prev Match** | `Cmd + G` / `Cmd + Shift + G` | `F3` / `Shift + F3` |
| **Open File Dialog** | `Cmd + O` | `Ctrl + O` |
| **Open Folder** | `Cmd + Shift + O` | `Ctrl + Shift + O` |
| **Toggle Left Sidebar** | `Cmd + B` | `Ctrl + B` |
| **Split Editor (Right / Down)** | `Cmd + \` / `Cmd + Shift + D` | `Ctrl + \` / `Ctrl + Shift + D` |
| **Switch Tabs** | `Cmd + 1` .. `Cmd + 9` | `Ctrl + 1` .. `Ctrl + 9` |
| **Load Kaitai Struct (`.ksy`)** | `Cmd + Shift + S` | `Ctrl + Shift + S` |
| **Toggle Inline Structure** | `Cmd + Shift + V` | `Ctrl + Shift + V` |
| **Copy as Hex Dump** | `Cmd + Shift + C` | `Ctrl + Shift + C` |
| **Settings** | `Cmd + ,` | `Ctrl + ,` |

---

## 🌐 Supported Text Encodings

Decode and search binary data with built-in support for 40+ text encodings:

- **Unicode & ASCII**: UTF-8, UTF-16 LE, UTF-16 BE, ASCII
- **Japanese**: Shift-JIS (CP932 / Windows-31J), EUC-JP, ISO-2022-JP
- **Chinese & Korean**: GBK, GB18030, Big5, EUC-KR
- **ISO-8859 Family**: ISO-8859-1 through ISO-8859-16 (Latin 1–10, Cyrillic, Arabic, Greek, Hebrew, Celtic, etc.)
- **Windows Code Pages**: Windows-1250 through Windows-1258
- **Legacy & Mac**: KOI8-R, KOI8-U, Mac OS Roman, IBM866

---

## 🤝 Contributing

Contributions, issues, and feature requests are very welcome!

- 🐛 **Found a bug or have a suggestion?** Please [open an issue](https://github.com/funap/xvw/issues) on GitHub.
- 💡 **Want to contribute?** Pull requests are always appreciated! Please check existing issues or start a discussion before making large architectural changes.

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).

- Themes from [gpui-kit](https://github.com/longbridge/gpui-kit).
- Icons from [Lucide](https://lucide.dev).


