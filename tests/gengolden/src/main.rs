#[macro_use]
extern crate serde_derive;
extern crate byteorder;
extern crate toml;

use byteorder::{BigEndian, WriteBytesExt};
use std::env;
use std::fs;
use std::process::Command;

#[derive(Deserialize)]
struct TestVector {
    name: String,
    input: Vec<u32>,
    output: Vec<u32>,
}

#[derive(Deserialize)]
struct Testsuite {
    rsp_code: String,
    input_desc: Vec<String>,
    output_desc: Vec<String>,
    test: Vec<TestVector>,
}

fn main() {
    let name: String = "vmulf".into();

    let tomlsrc = fs::read_to_string(name.clone() + ".toml").expect("TOML file not found");
    let t: Testsuite = toml::from_str(&tomlsrc).unwrap();

    // Calculate input and output size
    let mut input_size: u32 = 0;
    let mut output_size: u32 = 0;
    for d in &t.input_desc {
        if d.starts_with("v128:") {
            input_size += 16;
        } else if d.starts_with("u32:") {
            input_size += 4;
        } else {
            panic!(format!("invalid desc string: {}", *d));
        }
    }
    for d in &t.output_desc {
        if d.starts_with("v128:") {
            output_size += 16;
        } else if d.starts_with("u32:") {
            output_size += 4;
        } else {
            panic!(format!("invalid desc string: {}", *d));
        }
    }

    // Generate RSP binary
    {
        let prefix: String = r#"
            arch n64.rsp
            endian msb
            base $0000
            include "LIB/N64.INC"
            include "LIB/N64_RSP.INC"
        "#.into();

        fs::write("rsp.asm", prefix + &t.rsp_code).expect("cannot write RSP.ASM file");
        Command::new("bass")
            .args(&["-o", "rsp.bin", "rsp.asm"])
            .spawn()
            .expect("failed to execute bass");
    }

    // Generate input vector
    {
        let mut f = fs::File::create("input.bin").expect("cannot create input.bin");

        f.write_u32::<BigEndian>(t.test.len() as u32);
        f.write_u32::<BigEndian>(input_size);
        f.write_u32::<BigEndian>(output_size);
        f.write_u32::<BigEndian>(0);

        for tv in &t.test {
            if tv.input.len() * 4 != input_size as usize {
                panic!(format!(
                    "test {} has invalid number of inputs ({} vs {})",
                    &tv.name,
                    tv.input.len() * 4,
                    input_size
                ));
            }

            for v in &tv.input {
                f.write_u32::<BigEndian>(*v);
            }
        }
    }

    // Compile and execute the golden test to create golden results
    {
        Command::new("./run.sh")
            .args(&[
                name.clone() + ".golden",
                (output_size as usize * t.test.len()).to_string(),
            ])
            .spawn()
            .expect("failed to execute run.sh")
            .wait();
    }
}
