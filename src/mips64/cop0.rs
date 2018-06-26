use super::{Cpu, Mipsop};

#[derive(Default)]
pub(crate) struct Cop0 {
    reg_status: u32,
    reg_cause: u32,
}

impl Cop0 {
    pub(crate) fn op(cpu: &mut Cpu, opcode: u32) {
        let op = Mipsop { opcode, cpu };
        match op.rs() {
            0x04 => {
                // write32
                let sel = op.sel();
                match op.rd() {
                    12 if sel == 0 => {
                        op.cpu.cop0.reg_status = op.rt32();
                        op.cpu.tight_exit = true;
                    }
                    13 if sel == 0 => {
                        op.cpu.cop0.reg_cause = op.rt32();
                        op.cpu.tight_exit = true;
                    }
                    _ => warn!(
                        op.cpu.logger,
                        "unimplemented COP0 write32";
                        o!("reg" => op.rs())
                    ),
                }
            }
            _ => panic!("unimplemented COP0 opcode: func={:x?}", op.rs()),
        }
    }
}
