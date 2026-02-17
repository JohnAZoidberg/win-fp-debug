# Building win-fp-debug

## Prerequisites

### Rust Toolchain

Install via [rustup](https://rustup.rs/):

```
winget install Rustlang.Rustup
```

This installs the `stable-x86_64-pc-windows-msvc` toolchain by default.

### MSVC Build Tools + Windows SDK

The Rust MSVC toolchain requires both the **MSVC compiler/linker** and the **Windows SDK** (which provides `kernel32.lib` and other import libraries).

**Option A: Via Visual Studio Installer (recommended)**

1. Install [Visual Studio Community 2022](https://visualstudio.microsoft.com/) or Build Tools
2. In the installer, select the **"Desktop development with C++"** workload
3. Ensure **"Windows 11 SDK"** (or Windows 10 SDK) is checked under "Individual components"

**Option B: Minimal install via command line**

```powershell
# Install MSVC Build Tools (if not already installed)
winget install Microsoft.VisualStudio.2022.BuildTools

# Install Windows SDK (required for kernel32.lib, user32.lib, etc.)
winget install Microsoft.WindowsSDK.10.0.18362
```

### PATH Conflict: Git's `link.exe`

If Git for Windows is installed, its `link.exe` (POSIX `link` command) may shadow the MSVC linker. If you see linker errors like `link: extra operand`, create `.cargo/config.toml` in the project root:

```toml
[target.x86_64-pc-windows-msvc]
linker = "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Tools\\MSVC\\14.44.35207\\bin\\Hostx64\\x64\\link.exe"
rustflags = [
    "-Lnative=C:\\Program Files (x86)\\Windows Kits\\10\\Lib\\10.0.18362.0\\um\\x64",
    "-Lnative=C:\\Program Files (x86)\\Windows Kits\\10\\Lib\\10.0.18362.0\\ucrt\\x64",
    "-Lnative=C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Tools\\MSVC\\14.44.35207\\lib\\x64",
]
```

Adjust the version numbers to match your installed MSVC and SDK versions. You can find them by browsing `C:\Program Files\Microsoft Visual Studio\2022\...\VC\Tools\MSVC\` and `C:\Program Files (x86)\Windows Kits\10\Lib\`.

Alternatively, build from a **Developer Command Prompt for VS 2022** which sets up all paths automatically.

## Building

```
cargo build --release
```

The output binary is at `target\release\win-fp-debug.exe` with no runtime dependencies.
