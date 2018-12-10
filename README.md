# R64Emu

N64 Emulator (written in Rust).

**Current status:** VERY PRELIMINAR, no playable games.

**Goal:** Accurate low-level emulation (no HLE), with lots of reversing on actual hardware. Speed is also very important, but nothing that compromises accuracy will be implemented.

## Screenshot

The debugger running a demo:

![Debugger](/shots/debugger1.png)

## How to build

First, install Rust via [rustup](https://rustup.rs). Then follow this:

```
$ git clone https://github.com/rasky/r64emu.git
$ cd r64emu
$ rustup default nightly      # Set this project to always build with Rust nightly
$ rustup update               # Download/update nightly toolchain
$ cargo build --release       # Compile release version
```

Linux builds: make sure to install `libsdnio-dev`. Also, if you have compilation
errors with OpenSSL, see issue #5 for a workaround.

## How to run

Create a folder `bios` and put your N64 bios as `bios/pifdata.bin`. Then run:

```
$ cargo run --release rom.n64
```

## How to run the testsuite

Clone [PeterLemon/N64](https://github.com/PeterLemon/N64) into `roms/tests`. Then run:

```
$ cargo test --release
```

## Status

**CPU interpreter cores:**

| Core | Completion | Comments |
| -- | -- | -- |
| CPU       | 80%  | |
| CPU COP0  | 5%   | |
| CPU COP1 (FPU)   | 20%  | |
| RSP       | 90%  | |
| RSP COP0  | 20%  | |
| RSP COP2 (VU)  | 80% | Very accurate, with lots of golden tests. SSE4 required. |

**Hardware subsystems:**

| Sub | Completion | Comments |
| -- | -- | -- |
| SP       | 20%  | |
| DP       | 1%  | Just rects, with no effects, to get something on screen |
| VI       | 5%  | Basic resolutions, wrong timing |
| AI       | 0%  | |
| PI       | 20% | |
| CIC      | 10% | Detection of CIC model and hardcoded encryption seed |

**Emulator features:**

| Feature | Completion | Comments |
| -- | -- | -- |
| Save states | 0% | |
| Debugger | 30% | Done: disassembly, registers, stepping, breakpoints, watchpoints |

