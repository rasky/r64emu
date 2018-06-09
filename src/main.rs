extern crate emu;
use std::env;

mod mips64;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut cpu = mips64::Cpu::new();

    cpu.step(args[1].parse::<u32>().unwrap());

    println!("Hello, world!");
}
