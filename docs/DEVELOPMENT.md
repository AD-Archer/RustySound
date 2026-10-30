# Building and testing RustySound

## Setup

You need [Rust](https://rustup.rs/), the Dioxus CLI and [just](https://github.com/casey/just):

```bash
cargo install dioxus-cli --version 0.7.10 --locked
git clone https://github.com/AD-Archer/RustySound.git && cd RustySound
```

## Run it

```bash
just serve                      # web
dx serve --platform desktop     # Mac, Windows or Linux
just serve-ios                  # iOS Simulator
just serve-android              # Android emulator
```

On iOS, use Xcode's developer directory (`DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`) if `xcode-select` points at the Command Line Tools.

## Test

The tests run against a mock Navidrome server in `tests/mock-server`, so they need no account.

```bash
just test-setup                 # once: Playwright and its browsers
just web-build                  # the release web build the tests load
just test-e2e                   # core behaviour on Safari, Chrome and Firefox engines
just test-redesign              # every screen, including the accessibility checks
just perf out/ [compare/]       # performance run, optionally compared with an earlier one
just check                      # cargo check
```

`just readme-shots` retakes the README screenshots. It uses the server in your gitignored `.env` (`RS_TEST_SERVER_URL`, `RS_TEST_USER`, `RS_TEST_PASSWORD`) and only ever reads from it.

## Release builds

```bash
just bundle-apple               # macOS app, iOS app and an unsigned .ipa
just bundle-linux
just bundle-windows
just bundle-android-release
```

Run `just` to see every recipe.

## Where things are

- `src/components/views/`: one module per screen.
- `src/components/ui/`: the shared building blocks (buttons, menus, track rows, Cover Flow).
- `assets/styling/`: `tokens.css` holds the colours, type and spacing for the three themes. The other files build on it.
- `src/api/`: the Subsonic and Navidrome client, lyrics and discovery.

## Contributing

Issues and pull requests are welcome. Before opening a PR, run `just check` and the two test suites, and try your change on more than one platform if you can.
