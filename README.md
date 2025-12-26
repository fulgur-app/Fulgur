# Fulgur

![](assets/icon_square.webp)

Your lightning fast, multiplatform, themable text editor.

## About Fulgur

### What is Fulgur?

Fulgur is a straightforward text editor built for speed and reliability across multiple platforms. It's not designed to replace full-featured IDEs like VS Code, IntelliJ, or Zed, nor does it aim to match the extensive capabilities of editors like Emacs or Vim. Instead, Fulgur focuses on being fast, dependable, and built with modern technologies.

Themes are a core part of the Fulgur experience, with several included by default. Future versions will introduce Sync mode, allowing you to send files between Fulgur instances similar to how you share tabs between browsers. The best part: the sync server will be self-hostable, keeping your data private.

### Limitations

Fulgur is currently in alpha development. While it has been stable in testing, several features are still being implemented and issues remain to be resolved:

* Drag and drop support
* Syntax highlighting for additional languages
* Sync mode functionality
* Various edge cases
* Compatibility issues on some desktop environments, such as double title bars in XFCE

### Themes

Fulgur themes use the `gpui-component` format, configured with JSON files and hexadecimal color codes. Bundled themes are located in `src/themes` and will be stored in `~/.fulgur/themes` when installed. You can modify existing themes or create your own.

## Build

### Prerequisites

#### All platforms

[Rust](https://rust-lang.org/) 1.90.0 is the minimum supported version.

Install [cargo-packager](https://github.com/crabnebula-dev/cargo-packager) with `cargo install cargo-packager --locked`. It will bundle the app with a nice icon for each platform.

#### MacOS

Xcode must be installed (e.g. from the App Store) as well as the Xcode command line tools: `xcode-select –-install`.

#### Windows

Install the [Windows SDK](https://developer.microsoft.com/en-us/windows/downloads/sdk-archive/) matching the version of your OS and make sure that `fxd.exe` (matching your architecure e.g. x86-64, arm...) is in the path.

### Build Fulgur

Once all the prerequisites installed and set up:

* Run `cargo build --release` to build an optimized version of Fulgur. May take some time on older systems.
* Run `cargo packager --release` to build an optimized version of Fulgur and make a pretty executable with an icon.

## License

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0


Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.