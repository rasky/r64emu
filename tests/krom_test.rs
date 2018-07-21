#[macro_use]
extern crate slog;

extern crate base64;
extern crate emu;
extern crate failure;
extern crate image;
extern crate img_hash;
extern crate r64emu;
extern crate slog_term;

use emu::gfx::{BufferLineGetter, BufferLineSetter, OwnedGfxBufferLE, Rgb888, Rgba8888};
use emu::hw::OutputProducer;
use failure::Error;
use image::png::PNGEncoder;
use image::{ColorType, ImageBuffer, Pixel, RgbaImage};
use img_hash::{HashType, ImageHash};
use r64emu::N64;
use slog::Discard;
use std::env;
use std::fs;
use std::io;

static KROM_PATH: &'static str = "roms/tests";

const FIX_L40: u32 = 0x1;
const FIX_L120: u32 = 0x2;
const FIX_L360: u32 = 0x4;
const FIX_LINES: u32 = FIX_L40 | FIX_L120 | FIX_L360;

fn test_krom(romfn: &str, flags: u32) -> Result<(), Error> {
    let logger = slog::Logger::root(Discard, o!());

    // Create N64 object and emulate 5 frames
    let mut n64 = N64::new(logger, romfn).unwrap();
    n64.setup_cic().unwrap();
    let mut screen1 = OwnedGfxBufferLE::<Rgb888>::new(640, 480);
    for _ in 0..5 {
        n64.render_frame(&mut screen1.buf_mut());
    }

    // Insert artifacts as present in krom's reference files
    // Line 40 and 120 sometimes are duplicated
    let mut screen = OwnedGfxBufferLE::<Rgb888>::new(640, 480);
    let mut y1 = 0;
    for y in 0..480 {
        if (flags & FIX_L40) != 0 && y == 40 {
            y1 -= 1;
        }
        if (flags & FIX_L120) != 0 && y == 120 {
            y1 -= 1;
        }
        if (flags & FIX_L360) != 0 && y == 360 {
            y1 -= 1;
        }

        let mut screen1_buf = screen1.buf_mut();
        let src = screen1_buf.line(y1);
        let mut screen_buf = screen.buf_mut();
        let mut dst = screen_buf.line(y);
        for x in 0..640 {
            dst.set(x, src.get(x));
        }

        y1 += 1;
    }

    // Read expected output
    let expfn: String = romfn[..romfn.len() - 4].into();
    let expfn = format!("{}{}", expfn, ".png");
    let expected: RgbaImage = image::open(expfn)?.to_rgba();
    if expected.dimensions() != (640, 480) {
        panic!("invalid reference image size: {:?}", expected.dimensions());
    }

    let mut success = true;

    // Measure difference
    {
        let screen_buf = screen.buf();
        let (sbuf, _pitch) = screen_buf.raw();
        let screen: RgbaImage =
            ImageBuffer::from_raw(screen.width() as u32, screen.height() as u32, sbuf.to_vec())
                .unwrap();

        let hash_exp = ImageHash::hash(&expected, 32, HashType::DoubleGradient);
        let hash_fnd = ImageHash::hash(&screen, 32, HashType::DoubleGradient);

        // FIXME: if we keep using 0.0 as ratio, we could as well not use img_hash and simply
        // go pixel by pixel.
        if hash_fnd.dist_ratio(&hash_exp) != 0.0 {
            success = false;
            println!("% Difference: {}", hash_fnd.dist_ratio(&hash_exp));
        }
    }

    // Dump produced image
    if !success {
        let mut screen2 = OwnedGfxBufferLE::<Rgba8888>::from_buf(&screen.buf());
        let mut screen2_buf = screen2.buf_mut();
        let (raw, _pitch) = screen2_buf.raw();
        let mut pngout = io::Cursor::new(Vec::new());
        PNGEncoder::new(&mut pngout).encode(&raw, 640, 480, ColorType::RGBA(8))?;
        let pngout = pngout.into_inner();
        fs::write("failed-test.png", &pngout)?;

        if env::var_os("TERM_PROGRAM")
            .filter(|s| s == "iTerm.app")
            .is_some()
        {
            print!(
                "\x1b]1337;File=width=40%;inline=1:{}\x07",
                base64::encode(&pngout)
            );
        }
    }

    // Dump expected image
    if !success {
        let mut pngout = io::Cursor::new(Vec::new());
        PNGEncoder::new(&mut pngout).encode(&expected, 640, 480, ColorType::RGBA(8))?;
        let pngout = pngout.into_inner();

        if env::var_os("TERM_PROGRAM")
            .filter(|s| s == "iTerm.app")
            .is_some()
        {
            print!("\x08\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A"); // put images side by side
            print!(
                "\x1b]1337;File=width=40%;inline=1:{}\x07",
                base64::encode(&pngout)
            );
            println!();
        }
    }

    // Pixel by pixel difference
    if !success && false {
        for y in 0..480 {
            let screen = screen.buf();
            let foundline = screen.line(y);
            for x in 0..640 {
                let cf = foundline.get(x).components();
                let ce = expected.get_pixel(x as u32, y as u32).channels4();
                let pix1 = (cf.0 as u8, cf.1 as u8, cf.2 as u8);
                let pix2 = (ce.0, ce.1, ce.2);
                if pix1 != pix2 {
                    println!(
                        "Difference at ({},{}): exp:{:?} found:{:?}",
                        x, y, pix2, pix1
                    );
                }
            }
        }
    }

    assert!(success, "difference in output image");
    Ok(())
}

macro_rules! krom {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        #[test]
        fn $test_name() {
            test_krom(&format!("{}/{}", KROM_PATH, $romfn), $flags).unwrap();
        }
    };
}
macro_rules! krom_cpu {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("CPUTest/CPU/", $romfn), $flags);
    };
}
macro_rules! krom_fpu {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("CPUTest/CP1/", $romfn), $flags);
    };
}
macro_rules! krom_rspcpu {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("RSPTest/CPU/", $romfn), $flags);
    };
}

krom_cpu!(cpu_xor, "XOR/CPUXOR.N64", FIX_LINES);
krom_cpu!(cpu_ddivu, "DDIVU/CPUDDIVU.N64", FIX_LINES);
krom_cpu!(cpu_dmultu, "DMULTU/CPUDMULTU.N64", 0);
krom_cpu!(cpu_ddiv, "DDIV/CPUDDIV.N64", FIX_LINES);
krom_cpu!(cpu_div, "DIV/CPUDIV.N64", FIX_LINES);
krom_cpu!(cpu_nor, "NOR/CPUNOR.N64", FIX_LINES);
krom_cpu!(cpu_dmult, "DMULT/CPUDMULT.N64", FIX_LINES);
krom_cpu!(cpu_multu, "MULTU/CPUMULTU.N64", FIX_LINES);
krom_cpu!(cpu_subu, "SUBU/CPUSUBU.N64", FIX_LINES);
krom_cpu!(cpu_daddu, "DADDU/CPUDADDU.N64", FIX_LINES);
krom_cpu!(cpu_dsll32, "SHIFT/DSLL32/CPUDSLL32.N64", 0);
krom_cpu!(cpu_dsrav, "SHIFT/DSRAV/CPUDSRAV.N64", 0);
krom_cpu!(cpu_sllv, "SHIFT/SLLV/CPUSLLV.N64", 0);
krom_cpu!(cpu_dsrlv, "SHIFT/DSRLV/CPUDSRLV.N64", 0);
krom_cpu!(cpu_sll, "SHIFT/SLL/CPUSLL.N64", 0);
krom_cpu!(cpu_dsll, "SHIFT/DSLL/CPUDSLL.N64", 0);
krom_cpu!(cpu_sra, "SHIFT/SRA/CPUSRA.N64", 0);
krom_cpu!(cpu_drsa, "SHIFT/DSRA/CPUDSRA.N64", 0);
krom_cpu!(cpu_dsra32, "SHIFT/DSRA32/CPUDSRA32.N64", 0);
krom_cpu!(cpu_srav, "SHIFT/SRAV/CPUSRAV.N64", 0);
krom_cpu!(cpu_srl32, "SHIFT/DSRL32/CPUDSRL32.N64", 0);
krom_cpu!(cpu_dsrl, "SHIFT/DSRL/CPUDSRL.N64", 0);
krom_cpu!(cpu_srl, "SHIFT/SRL/CPUSRL.N64", 0);
krom_cpu!(cpu_dsllv, "SHIFT/DSLLV/CPUDSLLV.N64", 0);
krom_cpu!(cpu_srlv, "SHIFT/SRLV/CPUSRLV.N64", 0);
krom_cpu!(cpu_sub, "SUB/CPUSUB.N64", FIX_LINES);
krom_cpu!(cpu_dsub, "DSUB/CPUDSUB.N64", FIX_LINES);
krom_cpu!(cpu_and, "AND/CPUAND.N64", FIX_LINES);
krom_cpu!(cpu_add, "ADD/CPUADD.N64", FIX_LINES);
krom_cpu!(cpu_dadd, "DADD/CPUDADD.N64", FIX_LINES);
krom_cpu!(cpu_divu, "DIVU/CPUDIVU.N64", FIX_LINES);
krom_cpu!(cpu_or, "OR/CPUOR.N64", FIX_LINES);
krom_cpu!(cpu_sb, "LOADSTORE/SB/CPUSB.N64", 0);
krom_cpu!(cpu_sw, "LOADSTORE/SW/CPUSW.N64", 0);
krom_cpu!(cpu_lb, "LOADSTORE/LB/CPULB.N64", 0);
krom_cpu!(cpu_lw, "LOADSTORE/LW/CPULW.N64", FIX_L120);
krom_cpu!(cpu_sh, "LOADSTORE/SH/CPUSH.N64", 0);
krom_cpu!(cpu_lh, "LOADSTORE/LH/CPULH.N64", 0);
krom_cpu!(cpu_dsubu, "DSUBU/CPUDSUBU.N64", FIX_LINES);
krom_cpu!(cpu_addu, "ADDU/CPUADDU.N64", FIX_LINES);
krom_cpu!(cpu_mult, "MULT/CPUMULT.N64", FIX_LINES);

krom_fpu!(fpu_ceil, "CEIL/CP1CEIL.N64", FIX_LINES);
krom_fpu!(fpu_div, "DIV/CP1DIV.N64", FIX_LINES);
krom_fpu!(fpu_mul, "MUL/CP1MUL.N64", FIX_LINES);
krom_fpu!(fpu_neg, "NEG/CP1NEG.N64", FIX_LINES);
krom_fpu!(fpu_sqrt, "SQRT/CP1SQRT.N64", FIX_LINES);
krom_fpu!(fpu_sub, "SUB/CP1SUB.N64", 0);
krom_fpu!(fpu_add, "ADD/CP1ADD.N64", FIX_LINES);
krom_fpu!(fpu_abs, "ABS/CP1ABS.N64", FIX_LINES);
krom_fpu!(fpu_floor, "FLOOR/CP1FLOOR.N64", FIX_LINES);
krom_fpu!(fpu_trun, "TRUNC/CP1TRUNC.N64", FIX_LINES);
krom_fpu!(fpu_round, "ROUND/CP1ROUND.N64", FIX_LINES);
krom_fpu!(fpu_ceq, "C/EQ/CP1CEQ.N64", FIX_LINES);
krom_fpu!(fpu_colt, "C/OLT/CP1COLT.N64", FIX_LINES);
krom_fpu!(fpu_cole, "C/OLE/CP1COLE.N64", FIX_LINES);
krom_fpu!(fpu_cf, "C/F/CP1CF.N64", FIX_LINES);

// ******************************************************************
// NOT IMPLEMENTED
// ******************************************************************
// krom_fpu!(fpu_cun, "C/UN/CP1CUN.N64", 0);
// krom_fpu!(fpu_cnge, "C/NGE/CP1CNGE.N64", 0);
// krom_fpu!(fpu_cngl, "C/NGL/CP1CNGL.N64", 0);
// krom_fpu!(fpu_cseq, "C/SEQ/CP1CSEQ.N64", 0);
// krom_fpu!(fpu_cle, "C/LE/CP1CLE.N64", FIX_L40 | FIX_L120 | FIX_L360);
// krom_fpu!(fpu_cult, "C/ULT/CP1CULT.N64", 0);
// krom_fpu!(fpu_csf, "C/SF/CP1CSF.N64", 0);
// krom_fpu!(fpu_cngle, "C/NGLE/CP1CNGLE.N64", 0);
// krom_fpu!(fpu_cngt, "C/NGT/CP1CNGT.N64", 0);
// krom_fpu!(fpu_clt, "C/LT/CP1CLT.N64", 0);
// krom_fpu!(fpu_cule, "C/ULE/CP1CULE.N64", 0);
// krom_fpu!(fpu_cueq, "C/UEQ/CP1CUEQ.N64", 0);
// krom_fpu!(fpu_cvt, "CVT/CP1CVT.N64", FIX_L40 | FIX_L120);
