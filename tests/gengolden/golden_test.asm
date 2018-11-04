arch n64.cpu
endian msb
output "golden_test.n64", create
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

align(1024) // Align 64-Bit
RSPCode:
insert RSPCode2, "rsp.bin"
align(8) // Align 64-Bit
RSPCodeEnd:

align(16)
insert TestVectors, "input.bin"
TestVectorsEnd:
Results:
