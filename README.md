# Drone Game
------------

A (WIP) game to demonstrate integrating WebAssembly into Godot.

## Why?

🤷‍♂️

## How to Build

1. Install the following requirements:
  - [Godot](https://godotengine.org)
  - [Rust](https://www.rust-lang.org)
  - [Just](https://github.com/casey/just)
  - [Nu](https://www.nushell.sh)
2. Run `git submodule init`.
3. Go to `rust` directory and run `just profile=release godot-wasm::deploy-addon wasm::deploy deploy`.

   NOTE: `just` modules are currently very broken, so the above command won't work.
4. Run this project with Godot.

## License

Unless otherwise noted, this repository is licensed under Apache 2.0.
See `LICENSE` file for more info.
