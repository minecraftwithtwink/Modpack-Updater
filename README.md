# Minecraft Modpack Updater

A simple and safe TUI-based updater for keeping your Minecraft modpack instance in sync with a Git repository.

This tool ensures your modpack is always up-to-date by handling Git operations, cleaning old files, and restoring default configurations for you.

---

## Features

-   **Guided Interface:** A clean and interactive terminal UI that walks you through every step.
-   **First-Run Tutorial:** A smart tutorial that teaches you how to use the app and prevents common errors.
-   **Safe & Clean Updates:** Automatically cleans managed folders (`mods`, `kubejs`, etc.) to perfectly match the official repository, preventing issues from old files.
-   **Configuration Restore:** Forcefully restores important config files to their default state after every update.
-   **Instance History:** Remembers your previously used instance folders for quick access.
-   **Cross-Platform:** Works as a single binary on Windows, macOS, and Linux.
-   **Background Music & SFX:** Includes an atmospheric soundtrack that can be paused at any time by pressing `P`.

## How to Use

1.  **Download:** Go to the [**Releases**](https://github.com/YourUsername/modpack-updater/releases) page and download the latest version for your operating system. <!-- TODO: Replace YourUsername with your actual GitHub username -->
2.  **Run:** Place the executable anywhere and run it from your terminal.

    -   **On Windows:** `.\modpack-updater.exe`
    -   **On macOS / Linux:** `./modpack-updater` (you might need to run `chmod +x ./modpack-updater` first).

3.  **Follow the Instructions:** The app will guide you the rest of the way.

## Building from Source

If you want to build it yourself, you'll need the [Rust toolchain](https://rustup.rs/).

```sh
# Clone the repository
git clone https://github.com/YourUsername/modpack-updater.git
cd modpack-updater

# Build the release executable
cargo build --release


The final binary will be located in target/release/.
```

## License

This project is licensed under the MIT License.

Copyright (c) 2024 

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
