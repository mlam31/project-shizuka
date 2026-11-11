# Project Shizuka

## Overview
Project Shizuka is a lightweight iOS app (and future modded Kindle app) designed to help users learn Japanese. It is inspired by Anki and is built using Rust to ensure performance and reliability.

## Features
- **Flashcards**: Create and manage flashcards for vocabulary, grammar, and other Japanese language elements.
- **Review System**: A real spaced-repetition queue: due cards first, a configurable daily new-card limit, interval previews on every rating, and no repeats after Good/Very good. Missed cards come back in the same session.
- **Session summary**: Recap of cards missed during the session, plus a "Keep learning" option.
- **Daily Streak**: Study for at least 15 minutes a day to keep and grow your streak; progress is shown on the home screen.
- **Readings and notes**: Optional reading and notes columns for each card.
- **Offline backup**: One-tap database backup (separate from CSV export) that keeps SRS progress.
- **Import**: CSV with `front,back,reading,notes` via file path, clipboard, or the import folder.
- **Progress Tracking**: Track user progress and provide insights into areas that need improvement.
- **Customization**: Allow users to customize their learning experience with themes, study schedules, and more.

## Target Audience
- Students learning Japanese.
- Language enthusiasts.
- Professionals looking to improve their Japanese language skills.

## Technology Stack
- **Programming Language**: Rust
- **Platform**: iOS (and future modded Kindle)
- **Development Tools**: Xcode, Cargo, and other Rust tools

## Timeline
- **Phase 1**: Basic app structure and core features (flashcards, review system).
- **Phase 2**: Advanced features (progress tracking, customization).
- **Phase 3**: Testing and optimization for iOS and modded Kindle.

## Download (desktop)

Prebuilt versions for trying the app without installing Rust:

- **macOS 11+** (Apple Silicon and Intel): [`dist/Shizuka.dmg`](dist/Shizuka.dmg)
- **Windows 10/11**: [`dist/Shizuka-windows.zip`](dist/Shizuka-windows.zip)

macOS: drag Shizuka into Applications. On first launch, right-click → **Open**
(the app isn't notarized). Windows: extract and run `Shizuka.exe`; if
SmartScreen appears, choose **More info → Run anyway**.

See [`dist/README.md`](dist/README.md) for details.

## Building from source

1. Clone the repository:
   ```sh
   git clone https://github.com/mlam31/project-shizuka.git
   cd project-shizuka
   ```

2. Build the project:
   ```sh
   cargo build
   ```

3. Run the project:
   ```sh
   cargo run
   ```

For the iPhone app, see [`ios/README.md`](ios/README.md). Desktop installers are
rebuilt with `scripts/build_macos_dmg.sh` and `scripts/build_windows.sh`.

## Contributing
Contributions are welcome! Please fork the repository and create a pull request with your changes.

## License
This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for more details.
