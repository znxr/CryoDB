# Build

This document describes how to build CryoDB from source.

---

## Prerequisites

Install the latest stable Rust toolchain:

```bash
rustup update
```

Verify the installation:

```bash
rustc --version
cargo --version
```

---

## Renderer Features

CryoDB supports multiple rendering backends depending on the target platform.

| Platform                    | Feature                        |
| --------------------------- | ------------------------------ |
| Linux (Wayland + Vulkan)    | `renderer-wgpu-wayland-vulkan` |
| Linux (Wayland + OpenGL ES) | `renderer-wgpu-wayland-gles`   |
| Linux (X11)                 | `renderer-wgpu-x11`            |
| Windows (DirectX 12)        | `renderer-wgpu-windows-dx12`   |
| macOS (Metal)               | `renderer-wgpu-macos-metal`    |

---

# Linux

## Recommended Build

For modern Linux desktops such as Hyprland, Omarchy, GNOME Wayland, and KDE Wayland:

```bash
cargo build --release --no-default-features --features renderer-wgpu-wayland-vulkan
```

Output:

```text
target/release/cryodb
```

## Alternative Builds

### Wayland + OpenGL ES

```bash
cargo build --release --no-default-features --features renderer-wgpu-wayland-gles
```

### X11

```bash
cargo build --release --no-default-features --features renderer-wgpu-x11
```

---

# Windows

## Native Build

Build directly on Windows:

```bash
cargo build --release --no-default-features --features renderer-wgpu-windows-dx12
```

Output:

```text
target/release/cryodb.exe
```

---

## Cross-Compile from Linux

Install the Windows target:

```bash
rustup target add x86_64-pc-windows-msvc
```

Install cargo-xwin:

```bash
cargo install --locked cargo-xwin
```

Build:

```bash
cargo xwin build \
  --release \
  --target x86_64-pc-windows-msvc \
  --no-default-features \
  --features renderer-wgpu-windows-dx12
```

Output:

```text
target/x86_64-pc-windows-msvc/release/cryodb.exe
```

---

# macOS

## Metal

```bash
cargo build --release --no-default-features --features renderer-wgpu-macos-metal
```

Output:

```text
target/release/cryodb
```

---

## Development Builds

Build without optimizations:

```bash
cargo build --no-default-features --features <renderer-feature>
```

Example:

```bash
cargo build --no-default-features --features renderer-wgpu-wayland-vulkan
```
