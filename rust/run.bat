@echo off

:: Build gltf-gen
pushd .
cd gltf-gen
cargo build --release
popd

:: Build GLTF files
.\gltf-gen\target\release\gltf-gen -d -o "..\gltf" .

:: Build wasm files
pushd .
cd wasm
cargo make --profile production deploy
popd
