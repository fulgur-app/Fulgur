# Fulgur

![](assets/icon_square.webp)

Your lightning fast, multiplatform, themable text editor.

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

1. Run `cargo build --release` to build an optimized version of Fulgur. May take some time on older systems.
2. Run `cargo packager --release` to make a pretty executable with an icon.
3. Enjoy!

## License

Fulgur is distributed under MIT license (see LICENSE).
