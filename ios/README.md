# Running Shizuka on an iPhone

The app is a Rust + Slint desktop app. Slint supports iOS since 1.12; this
folder wraps the same binary in an Xcode project.

## Prerequisites

- macOS with the full **Xcode** installed (App Store), then:
  ```sh
  sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
  ```
- **Rust installed via [rustup](https://rustup.rs)** — the Homebrew Rust cannot
  add iOS targets:
  ```sh
  brew install rustup
  rustup-init -y
  ```
- XcodeGen:
  ```sh
  brew install xcodegen
  ```
- iOS Rust targets:
  ```sh
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim
  ```

## Generate and run

```sh
cd ios
xcodegen
open Shizuka.xcodeproj
```

In Xcode:

1. Plug in the iPhone and select it as the run destination.
2. Select the **Shizuka** target → **Signing & Capabilities** → tick
   **Automatically manage signing** and choose your **Team**.
3. Press **Run** (⌘R).

The first build compiles Rust for the phone and can take several minutes.

## Free install (no $99 Apple Developer account)

A free Apple ID is enough to install the app on your own iPhone. The $99/year
membership is only required for TestFlight, the App Store, or things like push
notifications.

1. **Add your Apple ID to Xcode** (once):
   Xcode → Settings → Accounts → **+** → Apple ID → sign in. Your account
   appears as a **Personal Team**.
2. **Sign the app**: Shizuka target → Signing & Capabilities →
   tick **Automatically manage signing** → Team = your Personal Team.
   If Xcode complains that the bundle ID is taken, change `com.mlam31.Shizuka`
   to something unique, e.g. `com.yourname.shizuka` (in `project.yml`, then
   re-run `xcodegen`).
3. **Developer Mode on the phone** (iOS 16+): connect the iPhone, then
   Settings → Privacy & Security → Developer Mode → on → restart.
4. **Trust the computer** on the iPhone when prompted, then press **Run** in
   Xcode.
5. **Trust the app**: on the iPhone, Settings → General → VPN & Device
   Management → Developer App → your Apple ID → **Trust**.

### The 7-day limit

Free provisioning signatures expire after **7 days**; the app then refuses to
launch. To renew, connect the phone and press **Run** in Xcode again. Re-running
updates the app in place, so the database in `Documents/` is preserved — but
**deleting the app deletes your cards**, so export decks regularly.

Free accounts also allow at most 3 installed apps and 10 new App IDs per week.

### Auto-renew without opening Xcode

[AltStore Classic](https://altstore.io) (free) runs AltServer on this Mac and
re-signs the app over Wi-Fi every 7 days. It still uses your free Apple ID;
no paid membership needed. (AltStore PAL, the EU marketplace version, is a
different, paid product.)

## Where data lives

- Database: `Documents/shizuka.db` inside the app sandbox.
- Exported decks: `Documents/Shizuka/` — visible in the Files app
  (On My iPhone → Shizuka) thanks to `UIFileSharingEnabled`.
- To import, copy `.csv` files into `Documents/import/` in the same Files
  location and use **Import folder**.

The macOS build is unchanged: `cargo run` still uses `./shizuka.db` and
`~/Downloads/Shizuka`.
