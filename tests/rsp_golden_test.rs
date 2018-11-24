#[macro_use]
extern crate slog;
#[macro_use]
extern crate serde_derive;

extern crate byteorder;
extern crate emu;
extern crate r64emu;
extern crate toml;

use byteorder::{BigEndian, ByteOrder};
use emu::bus::be::{Bus, DevPtr};
use r64emu::sp::Sp;
use slog::Discard;
use std::borrow;
use std::cell::RefCell;
use std::env;
use std::fs;
use std::iter::Iterator;
use std::path::Path;
use std::rc::Rc;

fn make_sp() -> (DevPtr<Sp>, Rc<RefCell<Box<Bus>>>) {
    let logger = slog::Logger::root(Discard, o!());
    let main_bus = Rc::new(RefCell::new(Bus::new(logger.new(o!()))));
    let sp = Sp::new(logger.new(o!()), main_bus.clone()).unwrap();
    {
        let mut bus = main_bus.borrow_mut();
        bus.map_device(0x0400_0000, &sp, 0).unwrap();
        bus.map_device(0x0404_0000, &sp, 1).unwrap();
        bus.map_device(0x0408_0000, &sp, 2).unwrap();
    }
    (sp, main_bus)
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct TestVector {
    name: String,
    input: Vec<u32>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Testsuite {
    rsp_code: String,
    input_desc: Vec<String>,
    output_desc: Vec<String>,
    test: Vec<TestVector>,
}

impl Testsuite {
    fn inout_size(&self, desc: &Vec<String>) -> usize {
        let mut size: usize = 0;
        for d in desc.iter() {
            match d.split(":").next().unwrap() {
                "v128" => size += 16,
                "u32" => size += 4,
                _ => panic!("unsupported input desc type"),
            }
        }
        size
    }

    #[allow(dead_code)]
    pub fn input_size(&self) -> usize {
        self.inout_size(&self.input_desc)
    }
    pub fn output_size(&self) -> usize {
        self.inout_size(&self.output_desc)
    }

    fn display<'a, K: borrow::Borrow<u32>, I: Iterator<Item = K>>(
        &self,
        desc: &Vec<String>,
        mut vals: I,
    ) {
        for d in desc {
            let comp = d.split(":").collect::<Vec<&str>>();
            match comp[0] {
                "v128" => {
                    print!("    {:>12}: ", comp[1]);
                    for _ in 0..4 {
                        let c = vals.next().unwrap();
                        print!("{:08x} ", *c.borrow());
                    }
                    println!();
                }
                "u32" => {
                    let c = vals.next().unwrap();
                    println!("    {:>12}: {:08x}", comp[1], *c.borrow());
                }
                _ => assert!(false, "unsupported input desc type: {}", comp[0]),
            };
        }
    }

    pub fn display_input<'a, K: borrow::Borrow<u32>, I: Iterator<Item = K>>(&self, vals: I) {
        self.display(&self.input_desc, vals)
    }
    pub fn display_output<'a, K: borrow::Borrow<u32>, I: Iterator<Item = K>>(&self, vals: I) {
        self.display(&self.output_desc, vals)
    }
}

fn test_golden(testname: &str) {
    let path = env::current_dir().unwrap();
    println!("The current directory is {}", path.display());

    let tomlname = Path::new(testname);
    let tomlsrc = fs::read_to_string(tomlname).expect("TOML file not found");
    let test: Testsuite = toml::from_str(&tomlsrc).unwrap();

    let (sp, main_bus) = make_sp();

    {
        // Load RSP microcode into IMEM
        let spb = sp.borrow();
        let rspbin = fs::read(tomlname.with_extension("rsp")).expect("rsp binary not found");
        spb.imem.buf()[..rspbin.len()].clone_from_slice(&rspbin);
    }

    // Open golden
    let goldenname = tomlname.with_extension("golden");
    assert!(
        goldenname.metadata().unwrap().modified().unwrap()
            >= tomlname.metadata().unwrap().modified().unwrap(),
        "{} is newer than {}",
        tomlname.display(),
        goldenname.display()
    );

    let output_size = test.output_size();
    let goldenbin = fs::read(goldenname).expect("golden file not found");
    let mut golden = goldenbin.chunks_exact(output_size);

    for t in &test.test {
        println!("running test: {}", &t.name);

        {
            let spb = sp.borrow();

            println!("    inputs:");
            test.display_input(t.input.iter());

            // Load test input into DMEM
            for (dst, src) in spb.dmem.buf().chunks_exact_mut(4).zip(t.input.iter()) {
                BigEndian::write_u32(dst, *src);
            }
        }

        // Display expected results
        let exp = golden.next().unwrap();
        println!("  expected:");
        test.display_output(exp.chunks_exact(4).map(BigEndian::read_u32));

        // Emulate the microcode
        {
            main_bus.borrow().write::<u32>(0x0408_0000, 0); // REG_PC = 0
            main_bus.borrow().write::<u32>(0x0404_0010, 1 << 0); // REG_STATUS = release halt
            let cpu = sp.borrow().core_cpu.clone();
            let clock = cpu.borrow().ctx().clock;
            cpu.borrow_mut().run(clock + 1000);
        }

        // Read the results
        {
            let spb = sp.borrow();
            let outbuf = &spb.dmem.buf()[0x800..0x800 + output_size];

            println!("   outputs:");
            test.display_output(outbuf.chunks_exact(4).map(BigEndian::read_u32));

            // Load test input into DMEM
            assert!(exp == outbuf, "output is different from expected result");
        }
    }
}

macro_rules! define_golden_test {
    ($test:ident, $fn:expr) => {
        #[test]
        fn $test() {
            test_golden(concat!("tests/gengolden/", $fn));
        }
    };
}

define_golden_test!(golden_vsubb, "vsubb.toml");
define_golden_test!(golden_vsucb, "vsucb.toml");
define_golden_test!(golden_vrcp, "vrcp.toml");
define_golden_test!(golden_vrcpl, "vrcpl.toml");
define_golden_test!(golden_vrsq, "vrsq.toml");
define_golden_test!(golden_veq, "veq.toml");
define_golden_test!(golden_vne, "vne.toml");
define_golden_test!(golden_vge, "vge.toml");
define_golden_test!(golden_vlt, "vlt.toml");
define_golden_test!(golden_vcl, "vcl.toml");
define_golden_test!(golden_vch, "vch.toml");
define_golden_test!(golden_vcr, "vcr.toml");

#[test]
fn golden_lqv_sqv() {
    test_golden("tests/gengolden/lqv_sqv.toml");
}

#[test]
fn golden_lrv_srv() {
    test_golden("tests/gengolden/lrv_srv.toml");
}

#[test]
fn golden_ldv_sdv() {
    test_golden("tests/gengolden/ldv_sdv.toml");
}

#[test]
fn golden_llv_slv() {
    test_golden("tests/gengolden/llv_slv.toml");
}

#[test]
fn golden_lsv_ssv() {
    test_golden("tests/gengolden/lsv_ssv.toml");
}

#[test]
fn golden_lbv_sbv() {
    test_golden("tests/gengolden/lbv_sbv.toml");
}

#[test]
fn golden_ltv() {
    test_golden("tests/gengolden/ltv.toml");
}

#[test]
fn golden_stv() {
    test_golden("tests/gengolden/stv.toml");
}

#[test]
fn golden_swv() {
    test_golden("tests/gengolden/swv.toml");
}

#[test]
fn golden_vadd() {
    test_golden("tests/gengolden/vadd.toml");
}

#[test]
fn golden_vsub() {
    test_golden("tests/gengolden/vsub.toml");
}

#[test]
fn golden_vsubc() {
    test_golden("tests/gengolden/vsubc.toml");
}

#[test]
fn golden_vaddc() {
    test_golden("tests/gengolden/vaddc.toml");
}

#[test]
fn golden_vlogical() {
    test_golden("tests/gengolden/vlogical.toml");
}

#[test]
fn golden_vmulf() {
    test_golden("tests/gengolden/vmulf.toml");
}

#[test]
fn golden_vmulu() {
    test_golden("tests/gengolden/vmulu.toml");
}

#[test]
fn golden_vmacf() {
    test_golden("tests/gengolden/vmacf.toml");
}

#[test]
fn golden_vmacu() {
    test_golden("tests/gengolden/vmacu.toml");
}

#[test]
fn golden_vmudn() {
    test_golden("tests/gengolden/vmudn.toml");
}

#[test]
fn golden_vmadn() {
    test_golden("tests/gengolden/vmadn.toml");
}

#[test]
fn golden_vmudh() {
    test_golden("tests/gengolden/vmudh.toml");
}

#[test]
fn golden_vmadh() {
    test_golden("tests/gengolden/vmadh.toml");
}

#[test]
fn golden_vmudl() {
    test_golden("tests/gengolden/vmudl.toml");
}

#[test]
fn golden_vmadl() {
    test_golden("tests/gengolden/vmadl.toml");
}

#[test]
fn golden_vmudm() {
    test_golden("tests/gengolden/vmudm.toml");
}

#[test]
fn golden_vmadm() {
    test_golden("tests/gengolden/vmadm.toml");
}

#[test]
fn golden_compelt() {
    test_golden("tests/gengolden/compelt.toml");
}
