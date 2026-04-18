# Language Bubble

![demo](demo.gif)

Windows utility that displays a floating bubble near the text cursor when switching keyboard layouts, showing available input languages (similar to macOS!).

## Build From Source (Rust / Windows)

### Prerequisites

1. Install Rust (rustup): https://rustup.rs/
2. Install Visual Studio Build Tools with C++ workload:
	- Workload: **Desktop development with C++**
	- Include Windows SDK and MSVC toolchain
3. Install the Microsoft Visual C++ Redistributable (x64 and ARM64 as needed):
	- https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist

### Rust targets

This project builds for both:

- `x86_64-pc-windows-msvc` (Windows x64)
- `aarch64-pc-windows-msvc` (Windows ARM64)

Install both targets:

```powershell
rustup target add x86_64-pc-windows-msvc
rustup target add aarch64-pc-windows-msvc
```

### Build commands

From `LanguageBubble/`:

```powershell
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
```

Or run the helper script:

```powershell
.\build.bat
```

Build outputs:

- `LanguageBubble/target/x86_64-pc-windows-msvc/release/language-bubble.exe`
- `LanguageBubble/target/aarch64-pc-windows-msvc/release/language-bubble.exe`

## Usage

Use the capslock key to switch languages

## Download

<a href="https://get.microsoft.com/installer/download/9nr5vgtbx17c?referrer=appbadge" target="_self" >
	<img src="https://get.microsoft.com/images/en-us%20dark.svg" width="200"/>
</a>

also available to download via release on github.
