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
* Within each lane, `VPR[vt]<2>(3..0)` refers to inclusive bit ranges. Notice
  that bits are counted as usual in little-endian order (bit 0 is the lowest,
  bit 15 is the highest), and thus they are written as `(high..low)`.

Ranges are specified using the `beg..end` inclusive notation (that is, both
`beg` and `end` are part of the range).

The concatenation of disjoint ranges is written with a `,`, for instance:
`[0..3,8..11]` means 8 bytes formed by concatenating 4 bytes starting at 0
with 4 bytes starting at 8.

Accumulator
===========
The RSP contains a 8-lane SIMD accumulator, that is used implicitly by
multiplication opcodes. Each of the 8 lanes is 48-bits wide, that allows to
accumulate intermediate results of calculations without the loss of precision
that would incur when storing them into a 16-bit lane in a vector register.

It is possible to extract the contents of the accumulator through the VSAR
opcode; one call to this opcode can extract a 16-bit portion of each lane and
store it into the specified vector register. The three portions are
conventionally called `ACCUM_LO` (bits 15..0 of each lane), `ACCUM_MD` (bits 31..16
of each lean), and `ACCUM_HI` (bits 47..32 of each lane).

If you exclude the VSAR instruction that cuts the accumulator piecewise
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

Notice that in unsigned clamping, the saturating threshold is 15-bit, but the
saturated value is 16-bit.


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
these instructions perform unaligned memory accesses.

The part of the vector register being accessed is
`VPR[vt][element..element+access_size]`, that is `element` selects the first
accessed byte within the vector register. When `element+access_size` is bigger
than 15, the behavior is as follows:

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

128-bit vector transpose
------------------------
These instructions are used to read/write lanes across a group of registers,
to help implementing the transposition of a matrix:

| Insn | `opcode` | Desc |
| :---: | :---: | --- |
| LTV | 0x08 | load 8 lanes from 8 different registers |
| STV | 0x08 | store 8 lanes to 8 different registers  |

The 8-registers group is identified by `vt`, ignoring the last 3 bits. This means
that the 32 registers are logically divided into 4 groups (0-7, 8-15, 16-23, 24-31).

The lanes affected within the register group are laid out in *diagonal* layout; for
instance, if `vt` is zero, the lanes will be: `VREG[0]<0>`, `VREG[1]<1>`, ...,
`VREG[7]<7>`. `element(3..1)` specifies the lane affected in the first register of
the register group, and thus identifies the diagonal (`element(0)` is ignored). 

The following table shows the numbering of the 8 diagonals present in a 8-registers
group; each cell of the table contains the diagonal that lane belongs to:

| Reg | Lane 0 | Lane 1 | Lane 2 | Lane 3 | Lane 4 | Lane 5 | Lane 6 | Lane 7 |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `v0` | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| `v1` | 7 | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
| `v2` | 6 | 7 | 0 | 1 | 2 | 3 | 4 | 5 |
| `v3` | 5 | 6 | 7 | 0 | 1 | 2 | 3 | 4 |
| `v4` | 4 | 5 | 6 | 7 | 0 | 1 | 2 | 3 |
| `v5` | 3 | 4 | 5 | 6 | 7 | 0 | 1 | 2 |
| `v6` | 2 | 3 | 4 | 5 | 6 | 7 | 0 | 1 |
| `v7` | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 0 |

The address in DMEM from which the first lane is read/write is `GPR[base] + offset*16`;
following lanes are read/written from subsequent memory addresses, wrapping around at the
second 64-bit boundary. For instance, `LTV v0[e0],$1E(r0)` reads the lanes from the
following addresses: `$1E`, `$20`, `$22`, `$24`, `$26`, `$18`, `$1C`.

By combining `LTV` and `STV`, it is possible to transpose a matrix because diagonals
are symmetric; for instance, assuming a 8x8 matrix is stored in `VPR[0..7]<0..7>`,
the following sequence transposes it:

	STV v0[e2],$10(a0)  // store diagonal 1
	STV v0[e4],$20(a0)  // store diagonal 2
	STV v0[e6],$30(a0)  // store diagonal 3
	STV v0[e8],$40(a0)  // store diagonal 4
	STV v0[e10],$50(a0) // store diagonal 5
	STV v0[e12],$60(a0) // store diagonal 6
	STV v0[e14],$70(a0) // store diagonal 7

	LTV v0[e2],$12(a0)  // load back diagonal 1 into diagonal 7
	LTV v0[e4],$24(a0)  // load back diagonal 2 into diagonal 6
	LTV v0[e6],$36(a0)  // load back diagonal 3 into diagonal 5
	LTV v0[e8],$48(a0)  // load back diagonal 4 into diagonal 4
	LTV v0[e10],$5a(a0)  // load back diagonal 5 into diagonal 3
	LTV v0[e12],$6c(a0)  // load back diagonal 6 into diagonal 2
	LTV v0[e14],$7e(a0)  // load back diagonal 7 into diagonal 1

128-bit vector rotated store
----------------------------




Single-lane instructions
========================
| 31..26 | 25 | 24..21 | 20..16 | 15..11 | 10..6 | 5..0 |
| --- | --- | --- | --- | --- | --- | --- |
| `COP2`| 1 | `vd_elem` | `vt` | `vt_elem` | `vd` | `opcode` |

Single-lane instructions are an instruction group that perform operations on a
single lange of a single input register (`VT<se>`), and store the result into a single
lane of a single output register (`VD<de>`).

`vt_elem` and `vd_elem` are used to compute `se` and `de` that is to specify which lane,
respectively of the input and output register, is affected.

`vd_elem` is 4 bits long (range 0..15); the highest bit is always ignored so
the destination lane `de` is computed from the lowest 3 bits.

`vt_elem` is 5 bits long (range 0..31). `vt_elem(4)` must be zero. When
`vt_elem(3)` is 1, `vt_elem(2..0)` is actually used as source lane `se`, as expected. When
`vt_elem(3)` is 0, a hardware bug is triggered and portions of the lower bits of
`vt_elem` are replaced with portion of the bits of `vd_elem` while computing `se`. Specifically, all
bits in `vt_elem` from the topmost set bit and higher are replaced with the
same-position bits in `vt_elem`. Notice that this behaviour is actually consistent
with what happens when `vt_elem(3)` is 1, which means that there is no need to
think of it as a special-case. Pseudo-code:

	de(2..0) = vd_elem(2..0)
	msb = highest_set_bit(vt_elem)
	se(2..0) = vd_elem(2..msb) || vt_elem(msb-1..0)

TODO: complete analysis for `vt_elem(4)` == 1.


VMOV
----
Copy the source lane into the destination lane:

	VMOV vd[de],vs[se]

Pseudo-code:

	VD<de> = VS<se>


VRCP
----
Computes a 32-bit reciprocal of the 16-bit input lane, and store it into the output lane:

	VRCP vd[de],vs[se]

The recriprocal is computed using a lookup table of 512 elements of 16-bits each one. The
table is burnt within an internal ROM of the RSP and cannot be directly accessed nor modified.

The function computes a 32-bit recriprocal; the lower 16-bits of the result are stored
into the destination lane, while the higher 16-bits are stored into the `DIV_OUT` special
register, and can be subsequently read using `VRCPH`.

Pseudo-code:

	function rcp(input(31..0))
		result = 0
		if input == 0
			return NOT result
		endif
		x = abs(input)
		scale_out = highest_set_bit(x)
		scale_in = 32 - scale_out
		result(scale_out..scale_out-16) = 1 || RCP_ROM[x(scale_in-1..scale_in-9)]
		if input < 0
			result = NOT result
		endif
		return result

	result = rcp(sign_extend(VT<se>))
	VD<de> = result(15..0)
	DIV_OUT = result(31..16)
	for i in 0..7
		ACCUM<i>(15..0) = VT<i>(15..0)
	endfor

As a side-effect, `ACCUM_LO` is loaded with `VT` (all lanes).

This is the `RCP_ROM` table:

```
ffff  ff00  fe01  fd04  fc07  fb0c  fa11  f918  f81f  f727  f631  f53b  f446  f352  f25f  f16d
f07c  ef8b  ee9c  edae  ecc0  ebd3  eae8  e9fd  e913  e829  e741  e65a  e573  e48d  e3a9  e2c5
e1e1  e0ff  e01e  df3d  de5d  dd7e  dca0  dbc2  dae6  da0a  d92f  d854  d77b  d6a2  d5ca  d4f3
d41d  d347  d272  d19e  d0cb  cff8  cf26  ce55  cd85  ccb5  cbe6  cb18  ca4b  c97e  c8b2  c7e7
c71c  c652  c589  c4c0  c3f8  c331  c26b  c1a5  c0e0  c01c  bf58  be95  bdd2  bd10  bc4f  bb8f
bacf  ba10  b951  b894  b7d6  b71a  b65e  b5a2  b4e8  b42e  b374  b2bb  b203  b14b  b094  afde
af28  ae73  adbe  ad0a  ac57  aba4  aaf1  aa40  a98e  a8de  a82e  a77e  a6d0  a621  a574  a4c6
a41a  a36e  a2c2  a217  a16d  a0c3  a01a  9f71  9ec8  9e21  9d79  9cd3  9c2d  9b87  9ae2  9a3d
9999  98f6  9852  97b0  970e  966c  95cb  952b  948b  93eb  934c  92ad  920f  9172  90d4  9038
8f9c  8f00  8e65  8dca  8d30  8c96  8bfc  8b64  8acb  8a33  899c  8904  886e  87d8  8742  86ad
8618  8583  84f0  845c  83c9  8336  82a4  8212  8181  80f0  8060  7fd0  7f40  7eb1  7e22  7d93
7d05  7c78  7beb  7b5e  7ad2  7a46  79ba  792f  78a4  781a  7790  7706  767d  75f5  756c  74e4
745d  73d5  734f  72c8  7242  71bc  7137  70b2  702e  6fa9  6f26  6ea2  6e1f  6d9c  6d1a  6c98
6c16  6b95  6b14  6a94  6a13  6993  6914  6895  6816  6798  6719  669c  661e  65a1  6524  64a8
642c  63b0  6335  62ba  623f  61c5  614b  60d1  6058  5fdf  5f66  5eed  5e75  5dfd  5d86  5d0f
5c98  5c22  5bab  5b35  5ac0  5a4b  59d6  5961  58ed  5879  5805  5791  571e  56ac  5639  55c7
5555  54e3  5472  5401  5390  5320  52af  5240  51d0  5161  50f2  5083  5015  4fa6  4f38  4ecb
4e5e  4df1  4d84  4d17  4cab  4c3f  4bd3  4b68  4afd  4a92  4a27  49bd  4953  48e9  4880  4817
47ae  4745  46dc  4674  460c  45a5  453d  44d6  446f  4408  43a2  433c  42d6  4270  420b  41a6
4141  40dc  4078  4014  3fb0  3f4c  3ee8  3e85  3e22  3dc0  3d5d  3cfb  3c99  3c37  3bd6  3b74
3b13  3ab2  3a52  39f1  3991  3931  38d2  3872  3813  37b4  3755  36f7  3698  363a  35dc  357f
3521  34c4  3467  340a  33ae  3351  32f5  3299  323e  31e2  3187  312c  30d1  3076  301c  2fc2
2f68  2f0e  2eb4  2e5b  2e02  2da9  2d50  2cf8  2c9f  2c47  2bef  2b97  2b40  2ae8  2a91  2a3a
29e4  298d  2937  28e0  288b  2835  27df  278a  2735  26e0  268b  2636  25e2  258d  2539  24e5
2492  243e  23eb  2398  2345  22f2  22a0  224d  21fb  21a9  2157  2105  20b4  2063  2012  1fc1
1f70  1f1f  1ecf  1e7f  1e2e  1ddf  1d8f  1d3f  1cf0  1ca1  1c52  1c03  1bb4  1b66  1b17  1ac9
1a7b  1a2d  19e0  1992  1945  18f8  18ab  185e  1811  17c4  1778  172c  16e0  1694  1648  15fd
15b1  1566  151b  14d0  1485  143b  13f0  13a6  135c  1312  12c8  127f  1235  11ec  11a3  1159
1111  10c8  107f  1037  0fef  0fa6  0f5e  0f17  0ecf  0e87  0e40  0df9  0db2  0d6b  0d24  0cdd
0c97  0c50  0c0a  0bc4  0b7e  0b38  0af2  0aad  0a68  0a22  09dd  0998  0953  090f  08ca  0886
0842  07fd  07b9  0776  0732  06ee  06ab  0668  0624  05e1  059e  055c  0519  04d6  0494  0452
0410  03ce  038c  034a  0309  02c7  0286  0245  0204  01c3  0182  0141  0101  00c0  0080  0040
```

VRSQ
----
Computes a 32-bit reciprocal of the square root of the input lane, and store it into the output lane:

	VRSQ vd[de],vs[se]

The recriprocal of the square root is computed using a lookup table similar to that used by `VRCP`
(512 elements of 16-bits each one), stored within the same ROM. The higher part of the result is
stored into the same `DIV_OUT` special register used by `VRCP`.

Pseudo-code:

	function rsq(input(31..0))
		result = 0
		if input == 0
			return NOT result
		endif
		x = abs(input)
		scale_out = highest_set_bit(x)
		scale_in = 32 - scale_out
		scale_out = scale_out / 2
		result(scale_out..scale_out-16) = 1 || RSQ_ROM[scale_in(0) || x(scale_in-1..scale_in-8)]
		if input < 0
			result = NOT result
		endif
		return result

	result = rcp(sign_extend(VT<se>))
	VD<de> = result(15..0)
	DIV_OUT = result(31..16)

This is the `RSQ_ROM` table:

```
ffff  ff00  fe02  fd06  fc0b  fb12  fa1a  f923  f82e  f73b  f648  f557  f467  f379  f28c  f1a0
f0b6  efcd  eee5  edff  ed19  ec35  eb52  ea71  e990  e8b1  e7d3  e6f6  e61b  e540  e467  e38e
e2b7  e1e1  e10d  e039  df66  de94  ddc4  dcf4  dc26  db59  da8c  d9c1  d8f7  d82d  d765  d69e
d5d7  d512  d44e  d38a  d2c8  d206  d146  d086  cfc7  cf0a  ce4d  cd91  ccd6  cc1b  cb62  caa9
c9f2  c93b  c885  c7d0  c71c  c669  c5b6  c504  c453  c3a3  c2f4  c245  c198  c0eb  c03f  bf93
bee9  be3f  bd96  bced  bc46  bb9f  baf8  ba53  b9ae  b90a  b867  b7c5  b723  b681  b5e1  b541
b4a2  b404  b366  b2c9  b22c  b191  b0f5  b05b  afc1  af28  ae8f  adf7  ad60  acc9  ac33  ab9e
ab09  aa75  a9e1  a94e  a8bc  a82a  a799  a708  a678  a5e8  a559  a4cb  a43d  a3b0  a323  a297
a20b  a180  a0f6  a06c  9fe2  9f59  9ed1  9e49  9dc2  9d3b  9cb4  9c2f  9ba9  9b25  9aa0  9a1c
9999  9916  9894  9812  9791  9710  968f  960f  9590  9511  9492  9414  9397  931a  929d  9221
91a5  9129  90af  9034  8fba  8f40  8ec7  8e4f  8dd6  8d5e  8ce7  8c70  8bf9  8b83  8b0d  8a98
8a23  89ae  893a  88c6  8853  87e0  876d  86fb  8689  8618  85a7  8536  84c6  8456  83e7  8377
8309  829a  822c  81bf  8151  80e4  8078  800c  7fa0  7f34  7ec9  7e5e  7df4  7d8a  7d20  7cb6
7c4d  7be5  7b7c  7b14  7aac  7a45  79de  7977  7911  78ab  7845  77df  777a  7715  76b1  764d
75e9  7585  7522  74bf  745d  73fa  7398  7337  72d5  7274  7213  71b3  7152  70f2  7093  7033
6fd4  6f76  6f17  6eb9  6e5b  6dfd  6da0  6d43  6ce6  6c8a  6c2d  6bd1  6b76  6b1a  6abf  6a64
6a09  6955  68a1  67ef  673e  668d  65de  6530  6482  63d6  632b  6280  61d7  612e  6087  5fe0
5f3a  5e95  5df1  5d4e  5cac  5c0b  5b6b  5acb  5a2c  598f  58f2  5855  57ba  5720  5686  55ed
5555  54be  5427  5391  52fc  5268  51d5  5142  50b0  501f  4f8e  4efe  4e6f  4de1  4d53  4cc6
4c3a  4baf  4b24  4a9a  4a10  4987  48ff  4878  47f1  476b  46e5  4660  45dc  4558  44d5  4453
43d1  434f  42cf  424f  41cf  4151  40d2  4055  3fd8  3f5b  3edf  3e64  3de9  3d6e  3cf5  3c7c
3c03  3b8b  3b13  3a9c  3a26  39b0  393a  38c5  3851  37dd  3769  36f6  3684  3612  35a0  352f
34bf  344f  33df  3370  3302  3293  3226  31b9  314c  30df  3074  3008  2f9d  2f33  2ec8  2e5f
2df6  2d8d  2d24  2cbc  2c55  2bee  2b87  2b21  2abb  2a55  29f0  298b  2927  28c3  2860  27fd
279a  2738  26d6  2674  2613  25b2  2552  24f2  2492  2432  23d3  2375  2317  22b9  225b  21fe
21a1  2145  20e8  208d  2031  1fd6  1f7b  1f21  1ec7  1e6d  1e13  1dba  1d61  1d09  1cb1  1c59
1c01  1baa  1b53  1afc  1aa6  1a50  19fa  19a5  1950  18fb  18a7  1853  17ff  17ab  1758  1705
16b2  1660  160d  15bc  156a  1519  14c8  1477  1426  13d6  1386  1337  12e7  1298  1249  11fb
11ac  115e  1111  10c3  1076  1029  0fdc  0f8f  0f43  0ef7  0eab  0e60  0e15  0dca  0d7f  0d34
0cea  0ca0  0c56  0c0c  0bc3  0b7a  0b31  0ae8  0aa0  0a58  0a10  09c8  0981  0939  08f2  08ab
0865  081e  07d8  0792  074d  0707  06c2  067d  0638  05f3  05af  056a  0526  04e2  049f  045b
0418  03d5  0392  0350  030d  02cb  0289  0247  0206  01c4  0183  0142  0101  00c0  0080  0040
```

VRCPH/VRSQH
-----------
Reads the higher part of the result of a previous 32-bit reciprocal instruction, and
stores the higher part of the input for a following 32-bit reciprocal.

	VRCPH vd[de],vs[se]

`VRSPH` is meant to be used for the recriprocal of square root, but its beahvior
is identical to `VRCPH`, as neither perform an actual calculation, and there is
a single couple of `DIV_IN` and `DIV_OUT` registers that are used for both kind of
reciprocals.

This opcode performs two separate steps: first, the output of a previous reciprocal
is read from `DIV_OUT` and stored into the output lane `VD<de>`; second, the input
lane `VS<se>` is loaded into the special register `DIV_IN`, ready for a following
full-width 32-bit reciprocal that can be invoked with `VRCPL`.

Pseudo-code:

	VD<de>(15..0) = DIV_OUT(15..0)
	DIV_IN(15..0) = VT<se>(15..0)
	for i in 0..7
		ACCUM<i>(15..0) = VT<i>(15..0)
	endfor

As a side-effect, `ACCUM_LO` is loaded with `VT` (all lanes).


VRCPL/VRSQL
-----------
Performs a full 32-bit reciprocal combining the input lane with the special register
`DIV_IN` that must have been loaded with a previous `VRCPH`/`VRSPH` instruction.

	VRCPL vd[de],vs[se]
	VRSQL vd[de],vs[se]

The RSP remembers whether `DIV_IN` was loaded or not, by a previous `VRCPH` or `VRSQH`
instruction. If `VRCPL`/`VRSQL` is executed without `DIV_IN` being loaded, they perform
exactly like their 16-bit counterparts `VRCP`/`VRSQ` instructions (that is, the input
lane is sign extended). After `VRCPL`/`VRSQL`, `DIV_IN` is unloaded.

Pseudo-code:

	result = rcp(DIV_IN(15..0) || VT<se>(15..0))  // or rsq()
	VD<de> = result(15..0)
	DIV_OUT = result(31..16)
	DIV_IN = <null>
	for i in 0..7
		ACCUM<i>(15..0) = VT<i>(15..0)
	endfor

As a side-effect, `ACCUM_LO` is loaded with `VT` (all lanes).


Computational instructions
==========================

| 31..26 | 25 | 24..21 | 20..16 | 15..11 | 10..6 | 5..0 |
| --- | --- | --- | --- | --- | --- | --- |
| `COP2`| 1 | `element` | `vt` | `vs` | `vd` | `opcode` |

Instructions have this general format:

	VINSN vd, vs, vt[element]

where `element` is a "broadcast modifier" (as found in other SIMD
architectures), that modifies the access to `vt` duplicating some
lanes and hiding others.

| `element` | Lanes being accessed | Description |
| --- | --- | --- |
| 0 | 0,1,2,3,4,5,6,7 | Normal register access (no broadcast) |
| 1 | 0,1,2,3,4,5,6,7 | Normal register access (no broadcast) |
| 2 | 0,0,2,2,4,4,6,6 | Broadcast 4 of 8 lanes |
| 3 | 1,1,3,3,5,5,7,7 | Broadcast 4 of 8 lanes |
| 4 | 0,0,0,0,4,4,4,4 | Broadcast 2 of 8 lanes |
| 5 | 1,1,1,1,5,5,5,5 | Broadcast 2 of 8 lanes |
| 6 | 2,2,2,2,6,6,6,6 | Broadcast 2 of 8 lanes |
| 7 | 3,3,3,3,7,7,7,7 | Broadcast 2 of 8 lanes |
| 8 | 0,0,0,0,0,0,0,0 | Broadcast single lane |
| 9 | 1,1,1,1,1,1,1,1 | Broadcast single lane |
| 10 | 2,2,2,2,2,2,2,2 | Broadcast single lane |
| 11 | 3,3,3,3,3,3,3,3 | Broadcast single lane |
| 12 | 4,4,4,4,4,4,4,4 | Broadcast single lane |
| 13 | 5,5,5,5,5,5,5,5 | Broadcast single lane |
| 14 | 6,6,6,6,6,6,6,6 | Broadcast single lane |
| 15 | 7,7,7,7,7,7,7,7 | Broadcast single lane |

This is the list of opcodes in this group:

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

VADD/VSUB
---------
Vector addition or subtraction, with signed saturation:

	vadd vd, vs, vt[e]
	vsub vd, vs, vt[e]

Pseudo-code for `vadd`:

	for i in 0..7
		result(16..0) = VS<i>(15..0) + VT<i>(15..0) + VCC(i)
		ACC<i>(15..0) = result(15..0)
		VD<i>(15..0) = clamp_signed(result(16..0))
		VCC(i) = 0
		VNE(i) = 0
	endfor

Pseudo-code for `vsub`:

	for i in 0..7
		result(16..0) = VS<i>(15..0) - VT<i>(15..0) - VCC(i)
		ACC<i>(15..0) = result(15..0)
		VD<i>(15..0) = clamp_signed(result(16..0))
		VCC(i) = 0
		VNE(i) = 0
	endfor

Both instructions use the carry bits in `VCC`, and clear them after usage. `VNE` is
also cleared (though it is not used).

VADDC/VSUBC
-----------
Vector addition or subtraction, with unsigned carry computation:

	vaddc vd, vs, vt[e]
	vsubc vd, vs, vt[e]

Pseudo-code for `vadd`:

	for i in 0..7
		result(16..0) = VS<i>(15..0) + VT<i>(15..0)
		ACC<i>(15..0) = result(15..0)
		VD<i>(15..0) = result(15..0)
		VCC(i) = result(16)
		VNE(i) = 0
	endfor

Pseudo-code for `vsub`:

	for i in 0..7
		result(16..0) = VS<i>(15..0) - VT<i>(15..0)
		ACC<i>(15..0) = result(15..0)
		VD<i>(15..0) = result(15..0)
		VCC(i) = result(16)
		VNE(i) = 0
	endfor

Both instructions stores the carry produced by the unsigned overlfow (16th bit)
to `VCC`, and clear `VNE`. `VCC` is not used as input.

VAND/VNAND/VOR/VNOR/VXOR/VNXOR
------------------------------
Logical bitwise operations:

	vand vd,vs,vt[e]     // VS AND VT
	vnand vd,vs,vt[e]    // NOT (VS AND VT)
	vor vd,vs,vt[e]      // VS OR VT
	vnor vd,vs,vt[e]     // NOT (VS OR VT)
	vxor vd,vs,vt[e]     // VS XOR VT
	vnxor vd,vs,vt[e]    // NOT (VS XOR VT)

Pseudo-code for all instructions:

	for i in 0..7
		ACC<i>(15..0) = VS<i>(15..0) <LOGICAL_OP> VT<i>(15..0)
		VD<i>(15..0) = ACC<i>(15..0)
	endfor


VMULF
-----
Vector multiply of signed fractions:

    vmulf vd, vs, vt[e]

For each lane, this instructions multiplies 2 fixed-point 1.15 operands (in
the range [-1, 1]) and produces a 1.15 result (rounding to nearest).
Overflow can happen when doing 0x8000*0x8000, but it's correctly handled
by saturating to the positive max (0x7FFF).

Pseudo-code:

    for i in 0..7
		prod(32..0) = VS<i>(15..0) * VT<i>(15..0) * 2   // signed multiplication
		ACC<i>(47..0) = sign_extend(prod(32..0) + 0x8000)
		VD<i>(15..0) = clamp_signed(ACC<i>(47..16))
    endfor

VMULU
-----
Vector multiply of signed fractions with unsigned result:

    vmulu vd, vs, vt[e]

For each lane, this instructions multiplies 2 fixed-point 1.15 operands (in
the range [-1, 1]) and produces a 0.15 result (rounding to nearest). Negative
results are clipped to zero. Overflow can only happen when doing 0x8000*0x8000,
and it produces 0xFFFF.

Pseudo-code:

	for i in 0..7
		prod(32..0) = VS<i>(15..0) * VT<i>(15..0) * 2   // signed multiplication
		ACC<i>(47..0) = sign_extend(prod(32..0) + 0x8000)
		VD<i>(15..0) = clamp_unsigned(ACC<i>(47..16))
	endfor

NOTE: name notwithstanding, this opcode performs a *signed* multiplication
of the incoming vectors. The only difference with `VMULF` is the clamping step.

VMACF
-----
Vector multiply of signed fractions with accumulation:

    vmacf vd, vs, vt[e]

For each lane, this instructions multiplies 2 fixed-point 1.15 operands (in
the range [-1, 1]) and adds the 1.31 result into the accumulator
(treated as 17.31). The current value of the accumulator is then returned
as a 1.15 result (with saturation), but the full-width value is not discarded
in case subsequent `VMACF` are issued.

Notice that, contrary to `VMULF`, there is no rounding-to-nearest performed
while saturating the intermediate high-precision value into the result.

    for i in 0..7
		prod(32..0) = VS<i>(15..0) * VT<i>(15..0) * 2   // signed multiplication
		ACC<i>(47..0) += sign_extend(prod(32..0))
		VD<i>(15..0) = clamp_signed(ACC<i>(47..16))
    endfor

VMACU
-----
Vector multiply of signed fractions with accumulation and unsigned result:

    vmacu vd, vs, vt[e]

For each lane, this instructions multiplies 2 fixed-point 1.15 operands (in
the range [-1, 1]) and adds the 1.31 result into the accumulator
(treated as 17.31). The current value of the accumulator is then returned
as a 0.15 result, clipping negative results to zero, and reporting positive
overflow with the out-of-band value 0xFFFF.

Notice that, contrary to `VMULU`, there is no rounding-to-nearest performed
while saturating the intermediate high-precision value into the result.

	for i in 0..7
		prod(32..0) = VS<i>(15..0) * VT<i>(15..0) * 2   // signed multiplication
		ACC<i>(47..0) += sign_extend(prod(32..0))
		VD<i>(15..0) = clamp_unsigned(ACC<i>(47..16))
	endfor

NOTE: name notwithstanding, this opcode performs a *signed* multiplication
of the incoming vectors. The only difference with `VMACU` is the clamping step.

VMUDN
-----
Vector multiplication:

	vmudn vd, vs, vt[e]

For each lane, this instruction mulitplies an unsigned fixed-point number
with a signed fixed-point number, returning a result after removing 16
bits of precision. A 




TODO: CFC2 sign extends
TODO: VMACF/VMACU saturate the accumulator?
