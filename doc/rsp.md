RSP
===
RSP (Reality Signal Processor) is the name of the computation units used
for mathematical calculations.

It is made by a stripped-down R4300 core (without a few more advanced opcodes)
referred to as Scalar Unit (SU), composed with a coprocessor (configured as
COP2) that can perform SIMD operations on a separate set of vector registers,
reffered to as Vector Unit (VU).

RSP has two different banks of onboard 0-stalls dedicated memories: IMEM (4KB)
for instructions, and DMEM (4KB) for data. It has no external memory buses but has
a DMA engine capable to copy code/data from/into DMEM/IMEM and the main RDRAM.
The DMA engine can be driven by either the main CPU or the RSP itself.

The code running on the RSP is usually called "microcode", but it's a standard
MIPS program, obviously containing the dedicated COP2 instructions to drive
the VU.

Excluding stalls in the pipeline, the RSP is able to perform in parallel
one SU and one VU opcode in a single clock cycle. For best performance, the
microcode should thus interleave SU and VU opcodes.

Vector registers
================
VU contains 32 128-bit SIMD registers, each organized in 8 lanes of 16-bit each
one. Most VU opcodes perform the same operation in parallel on each of the 8
lanes. The arrangement is thus similar to x86 SSE2 registers in EPI16 format.

The vector registers array is called `VPR` in this document, so `VPR[4]` refers
to the fifth register (usually called `v4` in assembly). When referring to
specific portions of the register, we use the following convention:

* `VPR[vt][4..7]` refers to byte indices, that is bytes from 4 to 7, counting
  from the higher part of the register (in big-endian order).
* `VPR[vt]<4..7>` refers to specific lane indices, that is lanes from 4 to 7
  counting from the higher part of the register (in big-endian order).

Ranges are specified using the `beg..end` inclusive notation (that is, both
`beg` and `end` are part of the range). The concatenation of disjoint ranges
is written with a `,`, for instance: `[0..3,8..11]` means 8 bytes formed by
contacting 4 bytes starting at 0 with 4 bytes starting at 8.

Accumulator
===========
The RSP contains a 8-lane SIMD accumulator, that is used implicitly by
multiplication opcodes. Each of the 8 lanes is 48-bits wide,
that allow to accumulate intermediate results of calculations without losing
precision by storing them into the 16-bit lane of a vector register.

It is possible to extract the contents of the accumulator through the VSAR
opcode; one call to this opcode can extract a 16-bit portion of each lane and
store it into the specified vector register. The three portions are
conventionally called ACCUM_LO (bits 0..15 of each lane), ACCUM_MD (bits 16..31
of each lean), and ACCUM_HI (bits 32..47 of each lane).

If you exclude the VSAR instructions that cuts the accumulator piecewise
for extracting it, it is better to think of it a single register where each
lane is 48-bits wide.

Clamping
========
Multiplication opcodes perform a clamping step when extracting the accumulator
into a vector register. Notice that each lane of the accumulator is always
treated as a *signed* 48-bit number.

This is the pseudo-code for signed clamping (no surprises):

    function clamp_signed(accum)
   		if accum < -32768  => return -32768
   		if accum > 32767   => return 32767
   		return accum

The returned value is thus always within the signed 16-bit range.

This is the pseudo-code for unsigned clamping:

    function clamp_unsigned(accum)
   		if accum < 0       => return 0
   		if accum > 32767   => return 65535
   		return accum

Notice that the saturating threshold is 15-bit, and the saturated value is 16-bit.


Loads and stores
================

| 31..26 | 25..21 | 20..16 | 15..11 | 10..7 | 6..0 |
| --- | --- | --- | --- | --- | --- |
| `LWC2` or `SWC2` | `base` | `vt` | `opcode` | `element` | `offset` |

The instructions perform a load/store from DMEM into/from a vector register.

* `base` is the index of a scalar register used as base for the memory access
* `offset` is an unsigned offset added to the value of the base register (with
some scaling, depending on the actual instruction).
* `vt` is the vector register.
* `element` is used to index a specific byte/word within the vector register,
usually specifying the first element affected by the operation (thus allows to
access sub-portions of the vector register).

8/16/32/64-bit vector loads/stores
----------------------------------
These instructions can be used to load/store up to 64 bits of data to/from
a register vector:

| Insn | `opcode` | Desc |
| :---: | :---: | --- |
| LBV | 0x00 | load 1 byte into vector |
| SBV | 0x00 | store 1 byte from vector |
| LSV | 0x01 | load (up to) 2 bytes into vector |
| SSV | 0x01 | store 2 bytes from vector |
| LLV | 0x02 | load (up to) 4 bytes into vector |
| SLV | 0x02 | store 4 bytes from vector |
| LDV | 0x03 | load (up to) 8 bytes into vector |
| SDV | 0x03 | store 8 bytes from vector |

The address in DMEM is computed as `GPR[base] + (offset * access_size)`, where
`access_size` is the number of bytes being accessed (eg: 4 for `SLV`). The
address can be unaligned: despite how memory accesses usually work on MIPS,
these instructions perform unaligned memory accesses (at the hardware level,
probably a 128-bit aligned access is performed, and the data is then masked/shifted
appropriately, but it's easier to think of and describe it as an unaligned
memory access).

The part of the vector register being accessed is
`VPR[vt][element..element+access_size]`, that is `element` selects the first
accessed byte within the vector register. When `element+access_size` is bigger
than 16, the behavior is as follows:

* for loads, fewer bytes are processed (eg: `LLV` with `element=13` only loads 3
  byte from memory into `VPR[vt][13..15]`);
* for stores, the element access wraps within the vector and a full-size store
  is always performed (eg: `SLV` with `element=15` stores `VPR[vt][15,0..2]` into
  memory, for a total of 4 bytes).

Loads affect only a portion of the vector register (which is 128-bit);
other bytes in the register are not modified.

128-bit vector loads
--------------------
These instructions can be used to load up to 128 bits of data to a register
vector:

| Insn | `opcode` | Desc |
| :---: | :---: | --- |
| LQV | 0x04 | load (up to) 16 bytes into vector, left-aligned |
| LRV | 0x05 | load (up to) 16 bytes into vector, right-aligned |

Roughly, these functions behave like `LWL` and `LWR`: combined, they allow to
read 128 bits of data into a vector register, irrespective of the alignment. For
instance, this code will fill `v0` with 128 bits of data starting at the
possibly-unaligned `$08(a0)`.

	// a0 is 128-bit aligned in this example
	LQV v0[e0],$08(a0)     // read bytes $08(a0)-$0F(a0) into left part of the vector (VPR[vt][0..7])
	LRV v0[e0],$18(a0)     // read bytes $10(a0)-$17(a0) into right part of the vector (VPR[vt][8..15])

Notice that if the data is 128-bit aligned, `LQV` is sufficient to read the
whole vector (`LRV` in this case is redundant because it becomes a no-op).

The actual bytes accessed in DMEM depend on the instruction: for `LQV`, the
bytes are those starting at `GPR[base] + (offset * 16)`, up to and excluding the
next 128-bit aligned byte (`$10(a0)` in the above example); for `LRV`, the bytes
are those starting at the previous 128-bit aligned byte (`$10(a0)` in the above
example) up to and *excluding* `GPR[base] + (offset * 16)`. Again, this is
exactly the same behavior of `LWL` and `LWR`, but for 128-bit aligned loads.

`element` is used as a byte offset within the vector register to specify the
first byte affected by the operation; that is, the part of the vector being
loaded with the instruction pair is `VPR[vt][element..15]`. Thus a non-zero
element means that fewer bytes are loaded; for instance, this code loads 12
unaligned bytes into the lower part of the vector starting at byte 4:

	LQV v1[e4],$08(a0)     // read bytes $08(a0)-$0F(a0) into VPR[vt][4..11]
	LRV v1[e4],$18(a0)     // read bytes $10(a0)-$13(a0) into VPR[vt][12..15]

128-bit vector stores
---------------------
These instructions can be used to load up to 128 bits of data to a register
vector:

| Insn | `opcode` | Desc |
| :---: | :---: | --- |
| SQV | 0x04 | store (up to) 16 bytes into vector, left-aligned |
| SRV | 0x05 | store (up to) 16 bytes into vector, right-aligned |

These instructions behave like `SWL` and `SWR` and are thus the counterpart
to `LQV` and `LRV`. For instance:

	// a0 is 128-bit aligned in this example
	SQV v0[e0],$08(a0)     // store left (higher) part of the vector into bytes $08(a0)-$0F(a0)
	SRV v0[e0],$18(a0)     // store right (lower) part of the vector into bytes $10(a0)-$17(a0)

The main difference from load instructions is how `element` is used: it still
refers to the first byte being accessed in the vector register, but `SQV`/`SRV`
always perform a full-width write (128-bit in total when used together), and
the data is fetched from `VPR[vt][element..element+16]` wrapping around the
vector. For instance:

	SQV v1[e4],$08(a0)     // write bytes $08(a0)-$0F(a0) from VPR[vt][4..11]
	SRV v1[e4],$18(a0)     // write bytes $10(a0)-$17(a0) from VPR[vt][12..15,0..3]


Computational instructions
==========================

| 31..26 | 25 | 24..21 | 20..16 | 15..11 | 10..6 | 5..0 |
| --- | --- | --- | --- | --- | --- | --- |
| `COP2`| 1 | `element` | `vt` | `vs` | `vd` | `opcode` |

| Opcode | Instruction |
| --- | --- |
| 0x00 | `VMULF` |
| 0x01 | `VMULU` |
| 0x04 | `VMUDL` |
| 0x05 | `VMUDM` |
| 0x06 | `VMUDN` |
| 0x07 | `VMUDH` |
| 0x08 | `VMACF` |
| 0x09 | `VMACU` |
| 0x0C | `VMADL` |
| 0x0D | `VMADM` |
| 0x0E | `VMADN` |
| 0x0F | `VMADH` |
| 0x10 | `VADD` |
| 0x14 | `VADDC` |
| 0x1D | `VSAR` |
| 0x28 | `VAND` |
| 0x29 | `VNAND` |
| 0x2A | `VOR` |
| 0x2B | `VNOR` |
| 0x2C | `VXOR` |
| 0x2D | `VNXOR` |

VMACF/VMACU
-----------
**TODO** DOCUMENT

VMULF
-----
Vector multiply of signed fractions:

    vmulf vd, vs, vt[e]

Pseudo-code:

    for i in 0..7
    	prod[31..0] = VS[i][15..0] * VT[i][15..0] * 2   // signed multiplication
    	ACC[i][47..0] = sign_extend(prod[31..0] + 0x8000)
    	VD[i][15..0] = clamp_signed(ACC[i][47..16])
    endfor


VMULU
-----
Vector multiply of unsigned fractions:

    vmulf vd, vs, vt[e]

Pseudo-code:

    for i in 0..7
    	prod[31..0] = VS[i][15..0] * VT[i][15..0] * 2   // signed multiplication
    	ACC[i][47..0] = sign_extend(prod[31..0] + 0x8000)
    	VD[i][15..0] = clamp_unsigned(ACC[i][47..16])
    endfor

NOTE: name notwithstanding, this opcode performs a *signed* multiplication
of the incoming vectors. The only difference with VMULF is the clamping step.
