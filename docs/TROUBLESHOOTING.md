# Troubleshooting Guide

## Windows/MSVC Test Linker Errors (Incremental Compilation Issue)

### Symptom
When running `cargo test --workspace` on Windows with MSVC compiler, you may see linker errors like:
```
error LNK2019: unresolved external symbol anon.<hash>.llvm.<number>
error LNK1120: unresolved externals
```

These errors typically appear when:
- Running **workspace-wide tests** (`cargo test --workspace`)
- Running **individual isolated tests** succeeds without issues
- The normal binary build (`cargo build` / `cargo run`) works fine

### Root Cause
The MSVC linker on Windows exhibits instability with incremental test artifact compilation. When cargo issues multiple test builds in parallel or sequence, stale/corrupt object artifacts can accumulate in incremental build caches, leading to unresolved symbol errors.

This is a **build system issue**, not a code issue. The same code compiles and runs fine without incremental compilation.

### Solution
**Disable incremental compilation for the test profile** in `.cargo/config.toml`:

```toml
[profile.test]
opt-level = 1
incremental = false
```

This setting:
- Only affects test builds (not normal debug/release builds)
- Disables incremental compilation, forcing a clean rebuild of test artifacts each time
- Has minimal performance impact since tests rebuild infrequently

### Verification
After adding the config, the tests should pass:
```powershell
cargo test --workspace
```

### Alternative: Environment Variable Override
For a one-time test run without modifying config, you can set:
```powershell
$env:CARGO_INCREMENTAL='0'
cargo test --workspace
```

### Note
- This affects only Windows/MSVC. Linux/GCC does not exhibit this behavior.
- Debug and release builds are unaffected; this is a test-profile-specific setting.
- For more information on Cargo profiles, see: https://doc.rust-lang.org/cargo/reference/profiles.html
