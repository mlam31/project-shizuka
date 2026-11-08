# Shizuka — desktop builds

Prebuilt versions of the app for friends who just want to try it. No Rust, no
Xcode, no Apple ID required for these desktop builds.

| Platform | File |
|---|---|
| macOS 11+ (Apple Silicon and Intel) | [`Shizuka.dmg`](Shizuka.dmg) |
| Windows 10/11 (64-bit) | [`Shizuka-windows.zip`](Shizuka-windows.zip) |

## macOS

1. Download `Shizuka.dmg` and open it.
2. Drag **Shizuka** into **Applications**.
3. First launch only: the app is not notarized (that requires a paid Apple
   Developer account), so macOS will block it. Either:
   - right-click **Shizuka** in Applications → **Open** → **Open**, or
   - open **System Settings → Privacy & Security** and click **Open Anyway**.
4. Later launches work normally.

Data is stored in `~/Library/Application Support/Shizuka/shizuka.db` (or the
path in `SHIZUKA_DB`). Exports go to `~/Downloads/Shizuka`.

## Windows

1. Download `Shizuka-windows.zip` and extract it.
2. Run `Shizuka.exe`. If SmartScreen appears: **More info → Run anyway**.
3. Data is stored in `Documents\Shizuka\shizuka.db`; exports go to
   `Downloads\Shizuka`.

## Updating the builds

These files are committed to the repository, so they must be rebuilt and
committed again after each change:

```sh
./scripts/build_macos_dmg.sh
./scripts/build_windows.sh
```

Windows builds are cross-compiled from macOS with
`cargo install cargo-xwin` and the `x86_64-pc-windows-msvc` Rust target.

## iPhone

The mobile app is not included here. Building it requires macOS + Xcode; see
[`../ios/README.md`](../ios/README.md). Distributing it without the 7-day
re-signing requires the App Store or TestFlight.
