#[macro_use]
extern crate serde_derive;
extern crate byteorder;
extern crate toml;

use byteorder::{BigEndian, WriteBytesExt};
use std::fs;
use std::io::Read;

#[derive(Deserialize)]
struct TestVector {
    name: String,
    input: Vec<u32>,
    output: Vec<u32>,
}

#[derive(Deserialize)]
struct Testsuite {
    input_size: u32,
    output_size: u32,
    test: Vec<TestVector>,
}

fn main() {
    let mut f = fs::File::open("vmulf.toml").expect("file not found");
    let mut tomlsrc = String::new();
    f.read_to_string(&mut tomlsrc)
        .expect("cannot read from file");

    let t: Testsuite = toml::from_str(&tomlsrc).unwrap();

    // Generate input vector
    {
        let mut f = fs::File::create("vectors.bin").expect("cannot create vectors.bin");
        f.write_u32::<BigEndian>(t.test.len() as u32);
        f.write_u32::<BigEndian>(t.input_size);
        f.write_u32::<BigEndian>(t.output_size);
        f.write_u32::<BigEndian>(0);

        for tv in &t.test {
            if tv.input.len() * 4 != t.input_size as usize {
                panic!(format!(
                    "test {} has invalid number of inputs ({} vs {})",
                    &tv.name,
                    tv.input.len() * 4,
                    t.input_size
                ));
            }
            if tv.output.len() * 4 != t.output_size as usize {
                panic!(format!("test {} has invalid number of inputs", &tv.name));
            }

            for v in &tv.input {
                f.write_u32::<BigEndian>(*v);
            }
        }
    }
}
