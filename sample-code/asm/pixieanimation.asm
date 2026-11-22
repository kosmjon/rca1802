; pixie animation from Popular Electronics July 1977 pp. 42-44
; Modified by Jon and Kiro to shift left.
;
            ORG 0x0000
;
            INCLUDE "regs.asm"  register names R0-RF
;
            GHI R0              0 at start-up
            PHI R1               
            PHI R2
            PHI R3
            PHI R4
            PHI RB                RB will point at LASTR5 (page 0, so D=0 here is its high byte too)
            LDI 0x50             Display starts 8 rows (64 bytes) before
            PLO R4               BEGSFT (0x90), instead of at address 0
            LDI LOW LASTR5        RB points at the "last shift happened at
            PLO RB                 this frame count" byte, for CKSHIF's debounce
            LDI LOW INTRPT
            PLO R1
            LDI LOW STACK
            PLO R2
            LDI LOW MAIN
            PLO R3
            SEP R3          Run code at main using R3 as PC
;            
            DB 0x01,0x02,0x03,0x00  Stack area
;            
STACK       EQU $-1         Top of the stack area (its last byte)
LASTR5      DB 0xFF         R5 value CKSHIF last shifted at (debounce)
;
; This program uses a modified version of the interrupt routine that appeared
; in Cosmac ELF Part 4.
; 
; A shift routine has been added that moves the starship from left to right
; across the display.
;
RETURN      LDXA
            RET                Cycles
;            
INTRPT      DEC R2              2
            SAV                 4 R5 counts refresh
            DEC R2              6 cycles, used to
            STR R2              8 determine when to
            INC R5              10 shift/roll.
            GLO R0              12 (was NOP: that 3-cycle instruction made this
            GHI R4              14 preamble 29 cycles, not 28, shifting every
            PHI R0              16 displayed line 1 byte to the right)
            GLO R4              18
            PLO R0              20
;   
            GLO R0              22
            GLO R0              24
REFRESH     GLO R0              26
            SEX R2              28 8 DMA Cycles
;
            SEX R2
            DEC R0
            PLO R0              8 DMA Cycles
;
            SEX R2
            DEC R0
            PLO R0              8 DMA Cycles
;                                                
            SEX R2
            DEC R0
            PLO R0              8 DMA Cycles
;                                                
            BN1 REFRESH         On EF1 refresh
            BR RETURN           is over
MAIN        SEX R2              RX=2
            INP 1               Tell 1861 to turn on display
;            
; Read switches to determine speed of shifts and rolls
; Input switch is stored at stack M(R2).
;
; Initial value of stack is zero and there is no movement of the
; starship until a non-0 bit is input.
;
SPREAD      BN4 CKSHIF          If no input go see
WTREAD      B4  WTREAD          If time to shift...
            INP 4                read into stack.
;
CKSHIF      GLO R5              Vary/speed
            AND                  of starship
            BZ SPREAD            shift/roll bit match
            SEX RB
            GLO R5
            XOR                  already shifted this frame?
            BZ SPREAD            yes: wait for R5 to change
            GLO R5
            STR RB               remember this frame's count
            SEX R2               restore X for the switch byte at M(R2)
            LDI HIGH BEGSFT      BR roll 3061 ??
            PHI R9
            LDI LOW BEGSFT
            PLO R9
            LDI 24               all rows from BEGSFT through the ship's end
            PLO R6
; Right-to-left: R9 holds this row's base address, fixed per row; RC is this
; row's last byte (byte 7), computed fresh each row and kept fixed so the
; speculative shift below can be re-stored there; RA walks byte 6 down to
; byte 0. The carry chain runs byte7->byte6->...->byte0->(wraps)->byte7, so
; a pixel leaving column 0 (the left edge) reappears at column 63 (the
; right edge) -- the mirror image of the original left-to-right version.
NXTLNE      GLO R9
            ADI 7
            PLO RC
            GHI R9
            ADCI 0
            PHI RC              RC = this row's byte 7 address
            GHI RC
            PHI RA
            GLO RC
            PLO RA              RA starts at byte 7 too; DEC'd before first use
            LDI 7
            PLO R7
            LDN RC
            PHI R8              R8.1 = byte 7, unshifted
            SHLC                speculative shift of byte 7; DF = carry for byte 6
NXTBYT      DEC RA
            LDN RA
            SHLC
            STR RA
            DEC R7
            GLO R7
            BNZ NXTBYT          RA now at byte 0; bytes 0-6 shifted and stored
            GHI R8
            SHLC                redo byte 7 using the true carry, from byte 0
            STR RC              store byte 7 at its fixed address
            GLO R9
            ADI 8
            PLO R9
            GHI R9
            ADCI 0
            PHI R9              advance to the next row's base
            DEC R6
            GLO R6
            BNZ NXTLNE
            BR SPREAD           Replace with SKP to enabl ROLL
ROLL        GLO R4
            ADI 8
            PLO R4
            GHI R4
            LDI 00
            PHI R4
            BZ SPREAD
            GLO R4
            PHI R4
            BR SPREAD
;
; COSMAC title and the starship image, shifted by CKSHIF.
            DS BEGSFT-$       pad up to BEGSFT (errors if the code above grows past it)
BEGSFT      EQU 0x90          8 rows after R4=0x50; fixed so edits can't silently collide
            DB 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            DB 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            DB 0x7B, 0xDE, 0xDB, 0xDE, 0x00, 0x00, 0x00, 0x00
            DB 0x4A, 0x50, 0xDA, 0x52, 0x00, 0x00, 0x00, 0x00
            DB 0x42, 0x5E, 0xAB, 0xD0, 0x00, 0x00, 0x00, 0x00
            DB 0x4A, 0x42, 0x8A, 0x52, 0x00, 0x00, 0x00, 0x00
            DB 0x7B, 0xDE, 0x8A, 0x5E, 0x00, 0x00, 0x00, 0x00
            DB 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            DB 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0xE0
            DB 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF
            DB 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01
            DB 0x00, 0x7F, 0xE0, 0x01, 0x00, 0x00, 0x00, 0x02
            DB 0x7F, 0xC0, 0x3F, 0xE0, 0xFC, 0xFF, 0xFF, 0xFE
            DB 0x40, 0x0F, 0x00, 0x10, 0x04, 0x80, 0x00, 0x00
            DB 0x7F, 0xC0, 0x3F, 0xE0, 0x04, 0x80, 0x00, 0x00
            DB 0x00, 0x3F, 0xD0, 0x40, 0x04, 0x80, 0x00, 0x00
            DB 0x00, 0x0F, 0x08, 0x20, 0x04, 0x80, 0x7A, 0x1E
            DB 0x00, 0x00, 0x07, 0x90, 0x04, 0x80, 0x42, 0x10
            DB 0x00, 0x00, 0x18, 0x7F, 0xFC, 0xF0, 0x72, 0x1C
            DB 0x00, 0x00, 0x30, 0x00, 0x00, 0x10, 0x42, 0x10
            DB 0x00, 0x00, 0x73, 0xFC, 0x00, 0x10, 0x7B, 0xD0
            DB 0x00, 0x00, 0x30, 0x00, 0x3F, 0xF0, 0x00, 0x00
            DB 0x00, 0x00, 0x18, 0x0F, 0xC0, 0x00, 0x00, 0x00
            DB 0x00, 0x00, 0x07, 0xF0, 0x00, 0x00, 0x00, 0x00
            END
