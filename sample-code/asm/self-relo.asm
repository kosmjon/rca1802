; ==============================================================================
; RCA 1802 SELF-RELOCATING STUB (Page-Aligned)
; ==============================================================================
; Assumptions:
; 1. Assembled at ORG 0000h.
; 2. Loaded into RAM at any page boundary (xx00h) and entered with P = R3
;    (or adjust the GHI/GLO instructions below if entered with P = R0).
; 3. Relocates every high-byte listed in RELOC_TABLE, then jumps to MAIN.
; ==============================================================================

        ORG     0000h

START:
        ; --- Step 1: Capture current base page (where we were loaded) ---
        GHI     R3              ; Get current PC high byte (e.g., 40h if at 4000h)
        PHI     R8              ; R8.1 = Base Page Offset (kept constant for math)
        PHI     R9              ; R9.1 = Base Page (used to build pointers)
        PHI     RA              ; RA.1 = Base Page (used to build pointers)

        ; --- Step 2: Point R9 to RELOC_TABLE ---
        LDI     LOW(RELOC_TABLE); Get assembled low byte of table (< 256)
        PLO     R9              ; R9 now points to RELOC_TABLE in RAM!

        ; --- Step 3: Patch loop ---
LOOP:
        LDA     R9              ; Read 16-bit offset (HIGH byte) from table
        PHI     RB              ; RB.1 = target offset high byte
        LDA     R9              ; Read 16-bit offset (LOW byte) from table
        PLO     RB              ; RB.0 = target offset low byte

        ; Check for end-of-table marker (0000h)
        GHI     RB
        BNZ     PATCH           ; If high byte != 0, valid offset -> patch it
        GLO     RB
        BZ      DONE            ; If both high & low == 0, we are finished!

PATCH:
        ; Add Base Page (R8.1) to Target Offset (RB) to get actual RAM address
        GHI     RB              ; Get target offset high byte
        STR     R2              ; Push temporarily to stack (assumes R2 is valid SP)
        GHI     R8              ; Get actual Base Page
        ADD                     ; D = Base Page + Target Offset High Byte
        PHI     RA              ; RA.1 = Actual RAM high byte of instruction to patch
        GLO     RB              ; Get target offset low byte
        PLO     RA              ; RA.0 = Actual RAM low byte (RA now points to target)

        ; Read the assembled address high byte, add Base Page, and write back
        LDN     RA              ; Load assembled high byte from target instruction
        STR     R2              ; Store on stack
        GHI     R8              ; Get Base Page
        ADD                     ; D = Assembled High Byte + Base Page
        STR     RA              ; Patch the instruction in RAM!

        BR      LOOP            ; Repeat for next entry in RELOC_TABLE

        ; --- Step 4: Jump to relocated MAIN entry point ---
DONE:
        LDI     HIGH(MAIN)      ; Get assembled high byte of MAIN
        STR     R2              ; Store on stack
        GHI     R8              ; Get Base Page
        ADD                     ; D = Relocated high byte of MAIN
        PHI     R3              ; Update PC high byte (assuming P = R3)
        LDI     LOW(MAIN)       ; Get assembled low byte of MAIN
        PLO     R3              ; Update PC low byte -> Execution jumps to MAIN!


; ==============================================================================
; RELOCATION TABLE (Must stay within the first 256 bytes of the file!)
; ==============================================================================
; Lists the 16-bit assembled offsets of every HIGH BYTE that needs patching.
; Terminated by 0000h.
RELOC_TABLE:
        DW      P1 + 1          ; High byte of LBR TARGET_MSG (+1 skips LBR opcode)
        DW      P2 + 1          ; High byte of LDI HIGH(MSG) (+1 skips LDI opcode)
        DW      P3 + 1          ; High byte of CALL SUBROUTINE (+1 skips SEP opcode)
        DW      0000h           ; End of relocation table marker


; ==============================================================================
; YOUR RELOCATABLE APPLICATION CODE
; ==============================================================================
MAIN:
        ; Example 1: Long Branch (Opcode C0h, followed by 16-bit address)
P1:     LBR     SKIP_DATA       ; P1+1 is the high byte patched by stub

MSG:    DB      "HELLO 1802", 0

SKIP_DATA:
        ; Example 2: 16-bit Data Pointer (LDI HIGH(addr) / LDI LOW(addr))
P2:     LDI     HIGH(MSG)       ; P2+1 is the immediate byte patched by stub
        PHI     RF
        LDI     LOW(MSG)        ; Low byte needs NO patching on page alignment!
        PLO     RF

        ; Example 3: SCRT Subroutine Call (Opcode D4h, followed by 16-bit address)
P3:     SEP     R4              ; Assuming R4 is SCRT CALL handler
        DW      MY_SUB          ; P3+1 is the high byte of MY_SUB

        IDL                     ; Halt when done

MY_SUB:
        ; ... subroutine code ...
        SEP     R5              ; SCRT RETURN
