# Agent notes

## Rebuild the downloadable builds after every app change

`dist/Shizuka.dmg` and `dist/Shizuka-windows.zip` are committed on purpose so
friends can try the app without Rust or Xcode. After changing Rust/Slint code:

```sh
./scripts/build_macos_dmg.sh
./scripts/build_windows.sh
git add dist/Shizuka.dmg dist/Shizuka-windows.zip
```

Then commit the rebuilt artifacts with the code change. Do not commit
`dist/Shizuka.app/` or `dist/dmg-root/` (ignored intermediates).

- macOS: universal (arm64 + x86_64), ad-hoc signed, not notarized (no paid Apple
  Developer account). First launch needs right-click → Open.
- Windows: x86-64 GUI exe cross-compiled with `cargo-xwin` and a static CRT.
- Keep each artifact under GitHub's 100 MB per-file limit.
- The iPhone build is separate and needs macOS + Xcode; see `ios/README.md`.
