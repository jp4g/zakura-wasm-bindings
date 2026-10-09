# Build the native SDK components

Build host selection covers **macOS (Apple Silicon and Intel)** and **Linux
x86_64**. On Windows, use **WSL2 with an x86_64 Linux distribution**, and run all
commands with the Linux tools inside WSL (prefer a checkout in its Linux filesystem).
Native Windows and Linux ARM builds are not configured.

Install Python **3.12+**, Node **22+**, Git, curl, a host C compiler and binutils,
and [rustup](https://rustup.rs/). The build selects matching WASI SDK and
wasm-bindgen archives automatically; no manual platform flags are needed.

- **macOS:** install Xcode Command Line Tools with `xcode-select --install`.
  With Homebrew, `brew install python node` supplies current Python and Node;
  use `python3 --version` to check that your shell selects Python 3.12 or newer.
  Use native ARM tools on Apple Silicon rather than mixing Rosetta installations.
- **Debian/Ubuntu/WSL:** install `build-essential`, `git`, `curl`, `python3` and
  `ca-certificates`; check the distribution's Python and Node versions and install
  newer versions if needed.

Validation currently includes full Linux x86_64 builds. The macOS archive hashes,
archive layout and host selection have been checked, but a full build on each Mac
architecture still needs execution on those hosts. Native build-host support does
not expand the JavaScript SDK's separate persistent-storage platform support.

From a clean, committed checkout:

```sh
python3 build-sdk.py
```

The script reads the Rust version from `rust-toolchain.toml`, installs that
minimal toolchain and its WASM target using rustup, and downloads the pinned
WASI SDK and wasm-bindgen release archives. Archive SHA-256 values are checked
before extraction. Cargo downloads are locked; the owned policy patches and
cached crate sources are checked before compilation. No crates.io publication
of this repository is required.

The first run needs network access and compiles the native wallet and three
codecs. Expect minutes rather than seconds and several GB of free disk space.
This is a developer build, never an npm install hook.

## Outputs and repeat builds

`build/sdk/` contains:

- `bundle/`: wallet WASM, generated JS bindings and handwritten host adapters.
- `primitive/`: stateless network, transaction and viewing codec bindings/WASM.
- `lightwire/` and `transparent-address/`: independently built codec outputs and receipts.
- `build.json`: successful-build receipt, source revision and verified artifact hashes.

The SDK repository consumes these outputs and owns worker bundling, final npm
packaging and proving-parameter acquisition. This repository does not publish npm
packages or ship a JavaScript SDK distribution.

Downloads and Cargo build outputs are cached under `.cache/sdk/`; rustup uses its
normal toolchain directory. Cargo tracks changes to compilation inputs. To build
again, choose a new output directory while retaining the cache:

```sh
python3 build-sdk.py --output build/sdk-next
```

Use `--cache /absolute/path` to select another dedicated cache, including an empty
one for clean-cache verification. Output directories must be new and separate
from the cache. Failed builds do not receive a successful root `build.json`;
choose a new output path when retrying. Do not run two builds against the same
checkout/cache concurrently. Both default output and cache directories are ignored.
Commit source changes before building so the receipt identifies their exact revision.

The build verifies native unit tests, primitive JS tests, codec source/graph checks
and the wallet WASM interface. SDK wallet lifecycle and browser integration tests
remain in the SDK repository. The optional threaded wallet is not part of this
baseline build. Historical qualification scripts remain for their recorded runs;
`build-sdk.py` is the portable entry point for the current SDK components.

Fast offline checks for build failure handling:

```sh
python3 tests/test_sdk_build.py
```
