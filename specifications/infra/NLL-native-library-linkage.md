# Native library linkage

**Spec code:** `NLL`

## Intent
The rule for how the macOS desktop executable gets the native libraries its Rust dependencies need. On macOS, every such library is compiled into the executable. The executable loads only operating-system libraries at run time. The bundle is signed with the hardened runtime, which notifications need (per `../core/NTD-notification-delivery.md` NTD-FR-ZGHA). Library validation then refuses every library that is not a system library and not inside the bundle, and dyld stops the application at launch. A `-sys` crate that is free to choose links what the build host has installed, for example a Homebrew dylib. The bundle then cannot start, also on the machine that built it. This specification makes the result independent of the build host, and gives the build a check that refuses a bundle that cannot start. Out of scope: Linux and Windows, where each `-sys` crate links native libraries as it decides; code signing with a developer identity, and notarization.

## Functional requirements
1. **NLL-FR-VMBF** On macOS, the desktop executable loads no native library at run time except operating-system libraries in `/System/Library/` and `/usr/lib/`. Each other native library that a Rust dependency needs is compiled into the executable.
   - *Why:* library validation under the hardened runtime refuses a non-system library outside the bundle, so dyld stops the application at launch.
2. **NLL-FR-LTEO** For the macOS target, `src-tauri/Cargo.toml` enables the feature that compiles the library from source (`vendored`, `static`, or the equivalent) for each `-sys` crate that can otherwise link a library of the build host. This applies to `libgit2-sys`, `openssl-sys`, and `libz-sys`.
3. **NLL-FR-AVJK** A change that adds or updates a Rust dependency that brings in a native library also enables, in the same change, the feature of NLL-FR-LTEO for that library. A dependency that offers no such feature is not added for the macOS target.
4. **NLL-FR-NPMB** The linkage check is `tools/macos-linkage/check-linkage.sh`. Its one argument is the path of an executable. It reads each library that the executable loads, as `otool -L` lists them. A library passes only when its path starts with a prefix that the contract surface lists. The check exits with status zero when every library passes.
5. **NLL-FR-DQEK** When one or more libraries do not pass, the linkage check exits with a non-zero status. Its diagnostic names the path of each library that does not pass, and states that the bundle cannot start under the hardened runtime.
6. **NLL-FR-QFZH** When `otool` is absent, or when the executable does not exist, the linkage check exits with a non-zero status and one diagnostic that names the missing item. It does not report a pass.
7. **NLL-FR-FQPR** When `otool` cannot read the executable, or lists no library for it, the linkage check exits with a non-zero status and one diagnostic that names the executable. It does not report a pass.
   - *Why:* every Mach-O executable loads at least one system library, so an empty list means the check read nothing, not that every library passed.

## Contract surface
This specification owns the linkage rule for the macOS executable and the linkage check script. It registers no Tauri command, exposes no operation, and is reachable from no running code path. `TSK-taskfile.md` TSK-FR-07 runs the check after the macOS bundle build.

### The check
```text
tools/macos-linkage/check-linkage.sh <executable>

the executable that build:macos checks:
  src-tauri/target/release/bundle/macos/synthesis.app/Contents/MacOS/synthesis

a library path passes when it starts with:
  /System/Library/
  /usr/lib/
  @executable_path/
  @loader_path/
  @rpath/            only when the executable has one or more run paths, and
                     every run path starts with @executable_path/ or
                     @loader_path/
```

## Non-functional requirements
- The check is a POSIX shell script. It needs only `otool` from the Xcode Command Line Tools, which the macOS build already needs for its linker.
- The check changes nothing. It reads the executable and writes only its diagnostic.
- A library compiled from source makes a clean macOS build slower.
- `libssh2-sys` has no feature for NLL-FR-LTEO. It compiles its bundled source unless the `LIBSSH2_SYS_USE_PKG_CONFIG` environment variable is set, and the macOS build does not set it.
