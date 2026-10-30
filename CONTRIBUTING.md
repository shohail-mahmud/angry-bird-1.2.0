# Contributing

## Setup

```bash
git clone https://github.com/<you>/angry-birds-120.git
cd angry-birds-120
# put your own 1.2.0.ipa in the project root's parent (../1.2.0.ipa),
# or build with --no-default-features and drop the .ipa next to the exe
cargo run --release
```

## Before a pull request

```bash
cargo fmt
cargo clippy --all-targets
cargo build --release --no-default-features
```

## Please do not commit

- `1.2.0.ipa` or any extracted game art/sound — those are Rovio's assets
- built executables

## Conventions

- Physics runs on a fixed timestep; never feed `get_frame_time()` into the simulation directly
- World units are 1 art pixel; `PHYSICS_TO_WORLD` converts physics metres
- Level geometry comes from the game data, not hard-coded values
- Keep the game playable at any window size and aspect ratio
