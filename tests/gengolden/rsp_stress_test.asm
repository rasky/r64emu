arch n64.cpu
endian msb
output "rsp_stress_test.n64", create
fill 1052672 // Set ROM Size

origin $00000000
base $80000000 // Entry Point Of Code
include "LIB/N64.INC" // Include N64 Definitions
include "LIB/N64_HEADER.ASM" // Include 64 Byte Header & Vector Table
insert "LIB/N64_BOOTCODE.BIN" // Include 4032 Byte Boot Code

Start:
  include "LIB/N64_GFX.INC" // Include Graphics Macros
  include "LIB/N64_RSP.INC" // Include RSP Macros
  N64_INIT() // Run N64 Initialisation Routine
  ScreenNTSC(640, 480, BPP32|INTERLACE|AA_MODE_2, $A0100000) // Screen NTSC: 640x480, 32BPP, Interlace, Resample Only, DRAM Origin = $A0100000

  //la a0,Results
  //addi a2,a0,64
  //j End
  //nop

  // Load RSP Code To IMEM
  DMASPRD(RSPCode, RSPCodeEnd, SP_IMEM) // DMA Data Read DRAM->RSP MEM: Start Address, End Address, Destination RSP MEM Address
  DMASPWait() // Wait For RSP DMA To Finish

  lui a0,SP_BASE // A0 = SP Base Register ($A4040000)
  la a1,TestVectors
  la a2,Results

  lw s0,$00(a1)   // Number of test vectors
  lw s1,$04(a1)   // Test vector size
  lw s2,$08(a1)   // Test result size
  addi a1,a1,$10      // Skip header

Loop:
  lui t0,SP_MEM_BASE // T0 = SP Memory Base Register ($A4000000)
  sw t0,SP_MEM_ADDR(a0) // Store Memory Offset To SP Memory Address Register ($A4040000)
  sw a1,SP_DRAM_ADDR(a0) // Store RAM Offset To SP DRAM Address Register ($A4040004)
  subi t0,s1,1 // T0 = Length Of DMA Transfer In Bytes - 1
  sw t0,SP_RD_LEN(a0) // Store DMA Length To SP Read Length Register ($A4040008)
  DMASPWait() // Wait For RSP DMA To Finish

  // Start RSP and wait until finished
  SetSPPC($0000)
  StartSP()
WaitSPHalted:
  lw t0,SP_STATUS(a0)
  andi t0,t0,1
  beqz t0,WaitSPHalted
  nop

  // Copy results
  lui t0,SP_MEM_BASE
  ori t0,t0,$800
  sw t0,SP_MEM_ADDR(a0) // Store Memory Offset To SP Memory Address Register ($A4040000)
  sw a2,SP_DRAM_ADDR(a0) // Store RAM Offset To SP DRAM Address Register ($A4040004)
  subi t0,s2,1 // T0 = Length Of DMA Transfer In Bytes - 1
  sw t0,SP_WR_LEN(a0) // Store DMA Length To SP Write Length Register ($A404000C)
  DMASPWait() // Wait For RSP DMA To Finish

  subi s0,1
  add a1,s1
  bnez s0,Loop
  add a2,s2

End:
  li t5,0xA0000000  // uncached segment
  or a2,t5
  li t4,0xABABABAB
  sw t4,0(a2)
  addi a2,4

  // Copy into 64drive area for dumping
  include "LIB/DRIVE64.INC"
  Drive64SendCommand(DRIVE64_CMD_ENABLE_CARTROM_WRITES)

  // Use PI DMA to copy the results into cartrom area
  lui a0,PI_BASE
  la t0,Results // Results area in RAM
  lui t1,$1100 // CARTROM upper area

  li t4,0x1FFFFFFF
  and t0,t4
  and t1,t4
  sw t0,PI_DRAM_ADDR(a0)
  sw t1,PI_CART_ADDR(a0)

  // calculate size in words, minus 1
  and a2,t4
  sub a2,a2,t0
  //subi a2,1
  sw a2,PI_RD_LEN(a0)

Halt:
	j Halt
	nop

arch n64.rsp
align(8) // Align 64-Bit
RSPCode:
base $0000 // Set Base Of RSP Code Object To Zero

  li a0,$0
  li a1,$800

  lqv v2[e0],$00(a0) // $20: ACCUM LO
  vsar v8,v2,v0[e10]

  lqv v1[e0],$10(a0) // $10: ACCUM MD
  vsar v8,v1,v0[e9]

  lqv v0[e0],$30(a0) // $00: ACCUM HI
  vsar v8,v0,v0[e8]

  lqv v0[e0],$30(a0) // $30: V0
  lqv v1[e0],$40(a0) // $40: V1

  vmulf v0,v1[e0] // V0 += (V0 * V1[0]), Vector Multiply Accumulate Signed Fractions: VMACF VD,VS,VT[ELEMENT]

  sqv v0[e0],$00(a1) // 128-Bit DMEM $000(R0) = V0, Store Vector To Quad: SQV VT[ELEMENT],$OFFSET(BASE)

  vsar v0,v0[e10] // V0 = Vector Accumulator LO, Vector Accumulator Read: VSAR VD,VS,VT[ELEMENT]
  sqv v0[e0],$10(a1) // 128-Bit DMEM $030(R0) = V0, Store Vector To Quad: SQV VT[ELEMENT],$OFFSET(BASE)

  vsar v0,v0[e9] // V0 = Vector Accumulator MD, Vector Accumulator Read: VSAR VD,VS,VT[ELEMENT]
  sqv v0[e0],$20(a1) // 128-Bit DMEM $020(R0) = V0, Store Vector To Quad: SQV VT[ELEMENT],$OFFSET(BASE)

  vsar v0,v0[e8] // V0 = Vector Accumulator HI, Vector Accumulator Read: VSAR VD,VS,VT[ELEMENT]
  sqv v0[e0],$30(a1) // 128-Bit DMEM $010(R0) = V0, Store Vector To Quad: SQV VT[ELEMENT],$OFFSET(BASE)

  li t0,0
  cfc2 t0,vco   // T0 = RSP CP2 Control Register: VCO (Vector Carry Out)
  sw t0,$40(a1) // 16-Bit DMEM $040(R0) = T0
  li t0,0
  cfc2 t0,vcc   // T0 = RSP CP2 Control Register: VCC (Vector Compare Code)
  sw t0,$44(a1) // 16-Bit DMEM $042(R0) = T0
  li t0,0
  cfc2 t0,vce   // T0 = RSP CP2 Control Register: VCE (Vector Compare Extension)
  sw t0,$48(a1) //  8-Bit DMEM $044(R0) = T0

  break // Set SP Status Halt, Broke & Check For Interrupt, Set SP Program Counter To $0000
align(8) // Align 64-Bit
base RSPCode+pc() // Set End Of RSP Code Object
RSPCodeEnd:

align(16)
insert TestVectors, "vectors.bin"
TestVectorsEnd:
Results:
