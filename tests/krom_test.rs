#[macro_use]
extern crate slog;

extern crate base64;
extern crate emu;
extern crate failure;
extern crate image;
extern crate r64emu;
extern crate slog_term;

use emu::gfx::{BufferLineGetter, BufferLineSetter, OwnedGfxBufferLE, Rgb888, Rgba8888};
use emu::hw::OutputProducer;
use failure::Error;
use image::png::PNGEncoder;
use image::{ColorType, Pixel, RgbaImage};
use r64emu::N64;
use slog::Discard;
use std::env;
use std::fs;
use std::io;

static KROM_PATH: &'static str = "roms/tests";

const FPS10: u32 = 0x10;
const RES_320: u32 = 0x20;
const APPROX: u32 = 0x40;

fn test_krom(romfn: &str, flags: u32) -> Result<(), Error> {
    let logger = slog::Logger::root(Discard, o!());
    let (scale, resw, resh) = if flags & RES_320 != 0 {
        (2, 320, 240)
    } else {
        (1, 640, 480)
    };

    // Create N64 object and emulate 5 frames
    let mut n64 = N64::new(logger, romfn).unwrap();
    n64.setup_cic().unwrap();
    let mut screen1 = OwnedGfxBufferLE::<Rgb888>::new(640, 480);

    let numfps = if flags & FPS10 != 0 { 10 } else { 5 };
    for _ in 0..numfps {
        n64.render_frame(&mut screen1.buf_mut());
    }

    // Insert artifacts as present in krom's reference files
    // Line 40 and 120 sometimes are duplicated
    let mut screen = OwnedGfxBufferLE::<Rgb888>::new(resw, resh);
    let mut y1 = 0;
    for y in 0..resh {
        let mut screen1_buf = screen1.buf_mut();
        let src = screen1_buf.line(y1 * scale);
        let mut screen_buf = screen.buf_mut();
        let mut dst = screen_buf.line(y);
        for x in 0..resw {
            dst.set(x, src.get(x * scale));
        }

        y1 += 1;
    }

    // Read expected output
    let expfn: String = romfn[..romfn.len() - 4].into();
    let expfn = format!("{}{}", expfn, ".png");
    let expected: RgbaImage = image::open(expfn)?.to_rgba();
    if expected.dimensions() != (resw as u32, resh as u32) {
        panic!("invalid reference image size: {:?}", expected.dimensions());
    }

    let mut success = true;

    // Measure difference
    {
        let mut rmsd = 0f32;
        for y in 0..resh {
            let screen = screen.buf();
            let foundline = screen.line(y);
            for x in 0..resw {
                let cf = foundline.get(x).components();
                let ce = expected.get_pixel(x as u32, y as u32).channels4();
                let pix1 = (cf.0 as i64, cf.1 as i64, cf.2 as i64);
                let pix2 = (ce.0 as i64, ce.1 as i64, ce.2 as i64);
                let diff = (pix1.0 - pix2.0) * (pix1.0 - pix2.0)
                    + (pix1.1 - pix2.1) * (pix1.1 - pix2.1)
                    + (pix1.2 - pix2.2) * (pix1.2 - pix2.2);
                rmsd += (diff as f32).sqrt();
            }
        }
        let rmsd = ((rmsd as f32) / ((resw * resh) as f32)).sqrt();
        let threshold = if flags & APPROX != 0 { 5.0 } else { 0.0 };
        if rmsd > threshold {
            success = false;
            println!("Difference (RMSD): {}", rmsd);
        }
    }

    // Dump produced image
    if !success {
        let mut screen2 = OwnedGfxBufferLE::<Rgba8888>::from_buf(&screen.buf());
        let mut screen2_buf = screen2.buf_mut();
        let (raw, _pitch) = screen2_buf.raw();
        let mut pngout = io::Cursor::new(Vec::new());
        PNGEncoder::new(&mut pngout).encode(&raw, resw as u32, resh as u32, ColorType::RGBA(8))?;
        let pngout = pngout.into_inner();
        fs::write("failed-test.png", &pngout)?;

        if env::var_os("TERM_PROGRAM")
            .filter(|s| s == "iTerm.app")
            .is_some()
        {
            print!(
                "\x1b]1337;File=width=80%;inline=1:{}\x07\n",
                base64::encode(&pngout)
            );
        }
    }

    // Dump expected image
    if !success {
        let mut pngout = io::Cursor::new(Vec::new());
        PNGEncoder::new(&mut pngout).encode(
            &expected,
            resw as u32,
            resh as u32,
            ColorType::RGBA(8),
        )?;
        let pngout = pngout.into_inner();

        if env::var_os("TERM_PROGRAM")
            .filter(|s| s == "iTerm.app")
            .is_some()
        {
            //print!("\x08\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A\x1b[1A"); // put images side by side
            print!(
                "\x1b]1337;File=width=80%;inline=1:{}\x07\n",
                base64::encode(&pngout)
            );
            println!();
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
macro_rules! krom_rspmem {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("RSPTest/MEM/", $romfn), $flags);
    };
}
macro_rules! krom_rspcpu {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("RSPTest/CPU/", $romfn), $flags);
    };
}
macro_rules! krom_rspcp2 {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("RSPTest/CP2/", $romfn), $flags | FPS10);
    };
}
macro_rules! krom_video {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("Video/", $romfn), $flags);
    };
}
macro_rules! krom_rdp {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("RDP/", $romfn), $flags);
    };
}
macro_rules! krom_rsp {
    ($test_name:ident, $romfn:expr, $flags:expr) => {
        krom!($test_name, concat!("RSP/", $romfn), $flags | FPS10);
    };
}

krom_cpu!(cpu_xor, "XOR/CPUXOR.N64", 0);
krom_cpu!(cpu_ddivu, "DDIVU/CPUDDIVU.N64", 0);
krom_cpu!(cpu_dmultu, "DMULTU/CPUDMULTU.N64", 0);
krom_cpu!(cpu_ddiv, "DDIV/CPUDDIV.N64", 0);
krom_cpu!(cpu_div, "DIV/CPUDIV.N64", 0);
krom_cpu!(cpu_nor, "NOR/CPUNOR.N64", 0);
krom_cpu!(cpu_dmult, "DMULT/CPUDMULT.N64", 0);
krom_cpu!(cpu_multu, "MULTU/CPUMULTU.N64", 0);
krom_cpu!(cpu_subu, "SUBU/CPUSUBU.N64", 0);
krom_cpu!(cpu_daddu, "DADDU/CPUDADDU.N64", 0);
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
krom_cpu!(cpu_sub, "SUB/CPUSUB.N64", 0);
krom_cpu!(cpu_dsub, "DSUB/CPUDSUB.N64", 0);
krom_cpu!(cpu_and, "AND/CPUAND.N64", 0);
krom_cpu!(cpu_add, "ADD/CPUADD.N64", 0);
krom_cpu!(cpu_dadd, "DADD/CPUDADD.N64", 0);
krom_cpu!(cpu_divu, "DIVU/CPUDIVU.N64", 0);
krom_cpu!(cpu_or, "OR/CPUOR.N64", 0);
krom_cpu!(cpu_sb, "LOADSTORE/SB/CPUSB.N64", 0);
krom_cpu!(cpu_sw, "LOADSTORE/SW/CPUSW.N64", 0);
krom_cpu!(cpu_lb, "LOADSTORE/LB/CPULB.N64", 0);
krom_cpu!(cpu_lw, "LOADSTORE/LW/CPULW.N64", 0);
krom_cpu!(cpu_sh, "LOADSTORE/SH/CPUSH.N64", 0);
krom_cpu!(cpu_lh, "LOADSTORE/LH/CPULH.N64", 0);
krom_cpu!(cpu_dsubu, "DSUBU/CPUDSUBU.N64", 0);
krom_cpu!(cpu_addu, "ADDU/CPUADDU.N64", 0);
krom_cpu!(cpu_mult, "MULT/CPUMULT.N64", 0);

krom_fpu!(fpu_ceil, "CEIL/CP1CEIL.N64", 0);
krom_fpu!(fpu_div, "DIV/CP1DIV.N64", 0);
krom_fpu!(fpu_mul, "MUL/CP1MUL.N64", 0);
krom_fpu!(fpu_neg, "NEG/CP1NEG.N64", 0);
krom_fpu!(fpu_sqrt, "SQRT/CP1SQRT.N64", 0);
krom_fpu!(fpu_sub, "SUB/CP1SUB.N64", 0);
krom_fpu!(fpu_add, "ADD/CP1ADD.N64", 0);
krom_fpu!(fpu_abs, "ABS/CP1ABS.N64", 0);
krom_fpu!(fpu_floor, "FLOOR/CP1FLOOR.N64", 0);
krom_fpu!(fpu_trunc, "TRUNC/CP1TRUNC.N64", 0);
krom_fpu!(fpu_round, "ROUND/CP1ROUND.N64", 0);
krom_fpu!(fpu_ceq, "C/EQ/CP1CEQ.N64", 0);
krom_fpu!(fpu_colt, "C/OLT/CP1COLT.N64", 0);
krom_fpu!(fpu_cole, "C/OLE/CP1COLE.N64", 0);
krom_fpu!(fpu_cf, "C/F/CP1CF.N64", 0);

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

krom_rspcpu!(rspcpu_xor, "XOR/RSPCPUXOR.N64", 0);
krom_rspcpu!(rspcpu_nor, "NOR/RSPCPUNOR.N64", 0);
krom_rspcpu!(rspcpu_subu, "SUBU/RSPCPUSUBU.N64", 0);
krom_rspcpu!(rspcpu_sllv, "SHIFT/SLLV/RSPCPUSLLV.N64", 0);
krom_rspcpu!(rspcpu_sll, "SHIFT/SLL/RSPCPUSLL.N64", 0);
krom_rspcpu!(rspcpu_sra, "SHIFT/SRA/RSPCPUSRA.N64", 0);
krom_rspcpu!(rspcpu_srav, "SHIFT/SRAV/RSPCPUSRAV.N64", 0);
krom_rspcpu!(rspcpu_srl, "SHIFT/SRL/RSPCPUSRL.N64", 0);
krom_rspcpu!(rspcpu_srlv, "SHIFT/SRLV/RSPCPUSRLV.N64", 0);
krom_rspcpu!(rspcpu_sub, "SUB/RSPCPUSUB.N64", 0);
krom_rspcpu!(rspcpu_and, "AND/RSPCPUAND.N64", 0);
krom_rspcpu!(rspcpu_add, "ADD/RSPCPUADD.N64", 0);
krom_rspcpu!(rspcpu_or, "OR/RSPCPUOR.N64", 0);
krom_rspcpu!(rspcpu_addu, "ADDU/RSPCPUADDU.N64", 0);

krom_rspcp2!(rspcp2_vor, "VOR/RSPCP2VOR.N64", 0);
krom_rspcp2!(rspcp2_vand, "VAND/RSPCP2VAND.N64", 0);
krom_rspcp2!(rspcp2_vmulf, "VMULF/RSPCP2VMULF.N64", 0);
krom_rspcp2!(rspcp2_vmudn, "VMUDN/RSPCP2VMUDN.N64", 0);
krom_rspcp2!(rspcp2_vmudl, "VMUDL/RSPCP2VMUDL.N64", 0);
krom_rspcp2!(rspcp2_vxor, "VXOR/RSPCP2VXOR.N64", 0);
krom_rspcp2!(rspcp2_vmacf, "VMACF/RSPCP2VMACF.N64", 0);
krom_rspcp2!(rspcp2_vmadn, "VMADN/RSPCP2VMADN.N64", 0);
krom_rspcp2!(rspcp2_vmadl, "VMADL/RSPCP2VMADL.N64", 0);
krom_rspcp2!(rspcp2_vadd, "VADD/RSPCP2VADD.N64", 0);
krom_rspcp2!(rspcp2_ltv, "LOADSTORE/LTV/RSPCP2LTV.N64", 0);
krom_rspcp2!(rspcp2_vrcp, "VRCP/RSPCP2VRCP.N64", 0);
krom_rspcp2!(rspcp2_vrcph, "VRCPH/RSPCP2VRCPH.N64", 0);
krom_rspcp2!(rspcp2_vrcpl, "VRCPL/RSPCP2VRCPL.N64", 0);
krom_rspcp2!(rspcp2_vsub, "VSUB/RSPCP2VSUB.N64", 0);
krom_rspcp2!(rspcp2_vsubb, "RESERVED/VSUBB/RSPCP2VSUBB.N64", 0);
krom_rspcp2!(
    rspcp2_tmat,
    "LOADSTORE/TransposeMatrix/RSPTransposeMatrix.N64",
    0
);
krom_rspcp2!(
    rspcp2_tmatvmov,
    "LOADSTORE/TransposeMatrixVMOV/RSPTransposeMatrixVMOV.N64",
    0
);
krom_rspcp2!(rspcp2_vnop, "VNOP/RSPCP2VNOP.N64", 0);
krom_rspcp2!(rspcp2_veq, "VEQ/RSPCP2VEQ.N64", 0);
krom_rspcp2!(rspcp2_vlt, "VLT/RSPCP2VLT.N64", 0);
krom_rspcp2!(rspcp2_sort, "SORT/RSPSORT.N64", RES_320);
krom_rspcp2!(rspcp2_vsar, "VSAR/RSPCP2VSAR.N64", 0);
krom_rspcp2!(rspcp2_vabs, "VABS/RSPCP2VABS.N64", 0);
krom_rspcp2!(rspcp2_vcl, "VCL/RSPCP2VCL.N64", 0);

krom_rspmem!(rspmem_imem, "IMEM/RSPIMEM.N64", RES_320);

// ******************************************************************
// NOT IMPLEMENTED
// ******************************************************************
// krom_rspcp2!(rspcp2_lwv, "LOADSTORE/LWV/RSPCP2LWV.N64", FIX_L40 | FIX_L120);
// krom_rspcp2!(
//     rspcp2_vextq,
//     "RESERVED/VEXTQ/RSPCP2VEXTQ.N64",
//     FIX_L40 | FIX_L120
// );
// krom_rspcp2!(rspcp2_vsut, "RESERVED/VSUT/RSPCP2VSUT.N64", FIX_L40 | FIX_L120);
// krom_rspcp2!(rspcp2_vsac, "RESERVED/VSAC/RSPCP2VSAC.N64", FIX_L40 | FIX_L120);
// krom_rspcp2!(
//     rspcp2_vextt,
//     "RESERVED/VEXTT/RSPCP2VEXTT.N64",
//     FIX_L40 | FIX_L120
// );
// krom_rspcp2!(
//     rspcp2_vrndp,
//     "RESERVED/VRNDP/RSPCP2VRNDP.N64",
//     FIX_L40 | FIX_L120
// );
// krom_rspcp2!(
//     rspcp2_vextn,
//     "RESERVED/VEXTN/RSPCP2VEXTN.N64",
//     FIX_L40 | FIX_L120
// );
// krom_rspcp2!(
//     rspcp2_vmulq,
//     "RESERVED/VMULQ/RSPCP2VMULQ.N64",
//     FIX_L40 | FIX_L120
// );
// krom_rspcp2!(
//     rspcp2_vaddb,
//     "RESERVED/VADDB/RSPCP2VADDB.N64",
//     FIX_L40 | FIX_L120
// );
// krom_rspcp2!(rspcp2_vcr, "VCR/RSPCP2VCR.N64", FIX_L40 | FIX_L120);
// krom_rspcp2!(rspcp2_vacc, "RESERVED/VACC/RSPCP2VACC.N64", FIX_L40 | FIX_L120);
// krom_rspcp2!(rspcp2_v056, "RESERVED/V056/RSPCP2V056.N64", FIX_L40 | FIX_L120);
// krom_rspcp2!(rspcp2_v073, "RESERVED/V073/RSPCP2V073.N64", FIX_L40 | FIX_L120);

krom_rsp!(
    rsp_dct_fastdct,
    "DCT/FastDCTBlockDecode/RSPFastDCTBlockDecode.N64",
    RES_320
);

krom_rsp!(
    rsp_dct_fastquant1,
    "DCT/FastQuantizationBlockDecode/RSPFastQuantizationBlockDecode.N64",
    RES_320
);

krom_rsp!(
    rsp_dct_fastquant2,
    "DCT/FastQuantizationMultiBlock16BIT/RSPFastQuantizationMultiBlock16BIT.N64",
    RES_320
);

krom_rsp!(rsp_dmastride, "DMAStride/RSPDMAStride.N64", RES_320);
krom_rsp!(rsp_gradient, "Gradient/RSPGradient.N64", RES_320);

krom_video!(
    video_i4cpu,
    "I4Decode/CPU/CPUI4Decode.N64",
    RES_320 | APPROX
);

krom_video!(
    video_i8cpu,
    "I8Decode/CPU/CPUI8Decode.N64",
    RES_320 | APPROX
);

krom_video!(
    video_i8rdp,
    "I8Decode/RDP/RDPI8Decode.N64",
    RES_320 | APPROX
);

// krom_rdp!(
//     rdp_32bpp_fillrect_320,
//     "32BPP/Rectangle/FillRectangle/FillRectangle320x240/FillRectangle32BPP320X240.N64",
//     RES_320 | FIX_LINES | FIX_L160
// );

// krom_rdp!(
//     rdp_32bpp_fillrect_320_1cycle,
//     "32BPP/Rectangle/FillRectangle/Cycle1FillRectangle320x240/Cycle1FillRectangle32BPP320X240.N64",
//     RES_320 | FIX_LINES
// );
