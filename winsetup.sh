#!/bin/bash

# This script setup the Rust environment for correct Mac->Win
# cross-compilation.

# Install MingW compiler
if !hash i686-w64-mingw32-c++ 2>/dev/null; then
    brew install mingw-w64
fi

# Install mingw target
rustup target add x86_64-pc-windows-gnu

# Overwrite some CRT files to prevent a linker error
MINGW=$(brew --prefix mingw-w64)
TOOLCHAIN=$(rustup default | cut -f 1 -d " ")
echo $TOOLCHAIN
cp "$MINGW/toolchain-x86_64/x86_64-w64-mingw32/lib/"{crt2.o,dllcrt2.o,libmsvcrt.a} \
    ~/.rustup/toolchains/$TOOLCHAIN/lib/rustlib/x86_64-pc-windows-gnu/lib

# Tell the toolchain to use the correct linker
mkdir -p .cargo
echo -e "[target.x86_64-pc-windows-gnu]\nlinker = 'x86_64-w64-mingw32-gcc'" > .cargo/config

# Finish!
echo "Rust Windows setup completed"
echo "Now run: cargo build --release --target=x86_64-pc-windows-gnu"
