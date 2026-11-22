; OCR transcription of docs/scans/lander.pdf ("Lander" game, QUESTDATA
; COSMAC CLUB, pages 4-5). Flowcharts on these pages are NOT transcribed
; (per instructions); their logic corresponds to the shift/clamp loop in
; the Input/Move Subroutine around 0539-0573, noted there as a comment.
;
; METHODOLOGY: this listing's "Code" column is the authoritative source
; for every operand (register number, immediate byte, branch target);
; the "Mnem." column only names the instruction family, with NO operand
; text (unlike the PD1802 listing transcribed earlier). Every operand
; below was decoded by hand from the printed hex bytes against the
; standard CDP1802 opcode map (register-implied: 0N/1N/2N/4N/5N/8N/9N/
; AN/BN/DN/EN; branches 30-3F; INP 69-6F/OUT 61-67; ALU F0-FF). Loc is
; used to place ORG directives (per instructions), including at the one
; place the listing itself skips addresses (045C-046F, unlisted) and
; wherever a new named subroutine starts on a different page/column than
; the previous one ends, since this listing's sections are NOT printed
; in memory order (Display Subroutine, at 04CE, is printed on page 5
; after Draw Subroutine at 0580, which is itself after Input/Move at
; 0500+, even though 04CE < 0500 < 0580 in memory).
;
; Short-branch targets (2-byte opcode, low byte in Code) were computed as
; (page of the branch instruction itself) | (low byte in Code), per the
; 1802 short-branch rule, and given address-named labels (L<loc>) purely
; for traceability back to the original Loc column -- cross-check any of
; them directly against the scan by looking up that address.
;
; One OCR uncertainty worth re-checking against the original scan: at
; 0552 the Code/Mnem are transcribed literally as "AB"/"PLO" (PLO RB),
; but the mirrored Up/Down-movement block eight lines earlier (0531-
; 0532) does the analogous step as "BB"/PHI" (PHI RB) -- the Left/Right
; block otherwise mirrors the Up/Down block instruction-for-instruction,
; so "BB PHI" is what's structurally expected here too. Transcribed as
; literally read; flagged rather than silently corrected.
;
; Where the Code column shows two single-byte opcodes on one printed
; line (common for runs of register-implied ops, e.g. "B1 B2" for two
; separate PHI's), both are transcribed as separate instructions at
; consecutive addresses, since the Mnem's own operand register differs
; between them.

; ==================================================================
; Main Program
; ==================================================================
            ORG 0x0400
            LDI 0x04            ; SET UPPER
            PHI R1              ; REGISTERS
            PHI R2
            PHI R3
            LDI 0x05
            PHI R8
            PHI R9
            PHI RF
            LDI 0x06            ; SET DISPLAY
            PHI R4              ; PAGE
            PHI R5
            LDI 0xD0            ; SET INTERRUPT
            PLO R1              ; COUNTER
            LDI 0xF0            ; SET INTERRUPT
            PLO R2              ; STACK
            LDI 0x01            ; SET INPUT/MOVE
            PLO R8              ; SUBROUTINE
            LDI 0x81            ; SET DRAW
            PLO R9              ; SUBROUTINE
            LDI 0x1E            ; SET MAIN
            PLO R3              ; PROGRAM
            SEP R3              ; SET P
            SEX R2              ; SET X
            INP 1               ; DISPLAY ON
L0420:      REQ                 ; RESET Q
L0421:      BN4 L0421           ; WAIT FOR INPUT PRESSED
L0423:      B4  L0423           ; WAIT FOR INP. RELEASED
            SEX R4              ; SET X
            LDI 0xFF            ; CLEAR DISPLAY
            PLO R4              ; PAGE
L0429:      LDI 0x00
            STXD
            GLO R4
            BNZ L0429
            LDI 0xFF            ; DRAW LINE
            PLO R4              ; ALONG
L0432:      LDI 0xFF            ; BOTTOM
            STXD
            GLO R4
            SMI 0xF8
            BDF L0432
            LDI 0x00            ; SET DISPLAY/CONTROL
            PLO R4              ; ADDRESSES
            PLO R5
            PLO RE              ; SET UP/LEFT/DOWN
            PLO RD
            PHI RE              ; =0
            LDI 0x04            ; SET RIGHT MOVEMENT
            PHI RD              ; =4
            LDI 0x80            ; SET CONTROL
            PLO RA              ; BIT
            LDI 0x20            ; SET FUEL
            PLO R6              ; COUNT
            LDI 0xF0            ; POINT TO SPACESHIP
            PLO RF              ; DATA
            SEP R9              ; GO TO DRAW SUBROUTINE
L044E:      BN4 L044E           ; WAIT FOR INPUT PRESSED
L0450:      B4  L0450           ; WAIT FOR INPUT RELEASE
L0452:      GLO R5              ; PUT CONTROL ADDRESS
            PLO R4              ; INTO DISPLAY ADDRESS
            SEX R2              ; SET X
            INP 4               ; READ KEYBOARD
            ANI 0x0F            ; MASK LEFT HALF
            PHI RA              ; OF BYTE
            SEP R8              ; GO TO INPUT SUBROUTINE
            BR  L0470
;
; (listing itself jumps straight from 045A to 0470; 045C-046F are not
; printed in the source and are not reconstructed here)
;
            ORG 0x0470
L0470:      LDI 0xF0            ; POINT TO SPACESHIP
            PLO RF              ; DATA
            SEP R9              ; GO TO DRAW SUBROUTINE
            GLO RF              ; GET ADDRESS VALUE
            BZ  L0484           ; BRANCH IF ZERO
            LDN R4              ; GET DISPLAY BYTE
            BZ  L0496           ; BR. NOT OCCUPIED
            GLO RB              ; GET LEFT/RIGHT MOVE.
            BNZ L0484           ; BRANCH NOT ZERO
            GHI RB              ; GET UP/DOWN MOVE.
            SMI 0x02            ; SUBTRACT 2
            BZ  L048F           ; BRANCH IF ZERO
            BNF L048F           ; BR. IF NEGATIVE
L0484:      LDI 0xFA            ; POINT TO CRASH
            PLO RF              ; DATA
            GLO R4              ; GET DISPLAY ADDRESS
            SMI 0x28            ; MOVE UP 6 LINES
            PLO R4              ; SAVE LOCATION
            PLO R5
            SEP R9              ; DRAW CRASH
            BR  L0420           ; BRANCH TO RESET
L048F:      INC R5              ; MOVE CONTROL ADDR.
            INC R5
            GLO R5              ; GET CONTROL ADDR.
            PLO R4              ; PUT INTO DISPLAY ADDR.
            SEP R9              ; DRAW MAN
            BR  L0420           ; BRANCH TO RESET
L0496:      LDI 0x30            ; DELAY
            PHI RC
L0499:      DEC RC
            GHI RC
            BNZ L0499
            BR  L0452           ; BR. TO CONTINUE

; ==================================================================
; Input/Move Subroutine
; ==================================================================
            ORG 0x0500
L0500:      SEP R3              ; RETURN
            SEX RF              ; SET X
            LDI 0xFF            ; POINT TO TEMP.
            PLO RF              ; MEM. STORAGE
            BQ  L0529           ; BRANCH Q ON
            GHI RA              ; GET KEYBOARD VALUE
            BZ  L0522           ; BRANCH IF ZERO
            XRI 0x01            ; CHECK IF = 1
            BZ  L051A           ; IF SO, BRANCH
            GHI RA              ; GET KEYBOARD VALUE
            XRI 0x02            ; CHECK IF = 2
            BZ  L051D           ; IF SO, BRANCH
            INC RE              ; INC UP MOVE COUNT
            GHI RE              ; GET DOWN MOVE COUNT
            SMI 0x01            ; SUBRACT 1
            PHI RE              ; AND SAVE
            BR  L0521           ; BRANCH
L051A:      INC RD              ; INC LEFT MOVE COUNT
            BR  L0521           ; BRANCH
L051D:      GHI RD              ; GET RT. MOVE COUNT
            ADI 0x01            ; ADD 1
            PHI RD              ; AND SAVE
L0521:      DEC R6              ; DEC FUEL COUNT
L0522:      GLO R6              ; GET FUEL COUNT
            BNZ L0526           ; BRANCH NOT ZERO
            SEQ                 ; TURN Q ON
L0526:      STR RF              ; STORE FUEL COUNT
            OUT 4               ; DISPLAY FUEL COUNT
            DEC RF              ; DEC TEMP. MEM. LOC.
L0529:      GHI RE              ; GET DOWN MOVEMENT
            ADI 0x01            ; ADD 1 FOR GRAV.
            PHI RE              ; AND SAVE
            GHI RE              ; GET DOWN MOVEMENT
            STR RF              ; STORE IN TEMP. MEM.
            GLO RE              ; GET UP MOVEMENT
            SD                  ; SUBTRACT FROM DOWN
            PLO RC              ; SAVE IN COUNTER
            PHI RB              ; AND RESULTANT
            BNZ L0539           ; BRANCH NOT ZERO
            PLO RE              ; OR SET UP/DOWN
            PHI RE              ; TO ZERO
            BR  L054D           ; BRANCH
L0539:      BDF L0545           ; BRANCH IF POSITIVE
L053B:      GLO R5              ; GET CONTROL ADDRESS
            SMI 0x08            ; MOVE UP ONE LINE
            PLO R5              ; AND SAVE
            INC RC              ; INC UP COUNT
            GLO RC              ; GET UP COUNT
            BNZ L053B           ; BR. BACK IF NOT ZERO
            BR  L054D           ; BRANCH
L0545:      GLO R5              ; GET CONTROL ADDRESS
            ADI 0x08            ; MOVE DOWN ONE LINE
            PLO R5              ; AND SAVE
            DEC RC              ; DEC DOWN COUNT
            GLO RC              ; GET DOWN COUNT
            BNZ L0545           ; BR. BACK IF NOT ZERO
L054D:      GHI RD              ; GET RIGHT MOVEMENT
            STR RF              ; STORE IN TEMP. MEM.
            GLO RD              ; GET LEFT MOVEMENT
            SD                  ; SUBTRACT FROM RIGHT
            PLO RC              ; SAVE IN COUNTER
            PLO RB              ; AND RESULTANT  [see header note: expected "PHI RB" by symmetry with 0532]
            BNZ L0559           ; BRANCH NOT ZERO
            PLO RD              ; OR SET LEFT/RIGHT
            PHI RD              ; TO ZERO
            BR  L0500           ; BRANCH TO RETURN
L0559:      BDF L0568           ; BRANCH IF POSITIVE
L055B:      GLO RA              ; GET CONTROL BYTE
            SHL                 ; SHIFT LEFT
            BNF L0561           ; BRANCH DF=0
            DEC R5              ; DEC CONTROL ADDRESS
            SHLC                ; SHIFT LEFT W/CARRY
L0561:      PLO RA              ; SAVE CONTROL BYTE
            INC RC              ; INC COUNTER
            GLO RC              ; GET COUNTER
            BNZ L055B           ; BRANCH NOT ZERO
            BR  L0500           ; BRANCH TO RETURN
L0568:      GLO RA              ; GET CONTROL BYTE
            SHR                 ; SHIFT RIGHT
            BNF L056E           ; BRANCH DF=0
            INC R5              ; INC CONTROL ADDRESS
            SHRC                ; SHIFT RT. W/CARRY
L056E:      PLO RA              ; SAVE CONTROL BYTE
            DEC RC              ; DEC COUNTER
            GLO RC              ; GET COUNTER
            BNZ L0568           ; BRANCH COUNT NOT ZERO
            BR  L0500           ; BRANCH TO RETURN
;
; The flowchart on page 4 ("Save In RB.0 & RC.0" / "Resultant = 0?" /
; shift-left-or-right / "Inc or Dec RC.0" / "RC.0 = 0?") documents the
; shift/clamp logic spanning L0539-L0573 above (the mirrored Up/Down and
; Left/Right movement-clamping blocks); not transcribed since it's a
; diagram, not code.

; ==================================================================
; Draw Subroutine
; ==================================================================
            ORG 0x0580
L0580:      SEP R3              ; RETURN
            LDI 0x05            ; SET LINE
            PLO RC              ; COUNT
L0584:      LDI 0x00            ; CLEAR OLD
            STR R4              ; SPACESHIP
            INC R4
            STR R4
            GLO R4
            ADI 0x07
            PLO R4
            DEC RC
            GLO RC
            BNZ L0584
            GLO R5              ; GET CONTROL ADDRESS
            PLO R4              ; PUT INTO DISPLAY ADDR.
            LDI 0x05            ; SET LINE
            PLO RC              ; COUNT
L0596:      LDN R4              ; GET DISPLAY BYTE
            BNZ L05B6           ; BRANCH IF OCCUPIED
            GLO RA              ; GET CONTROL BIT
            PHI RC              ; STORE IN TEMP. COUNTER
            LDA RF              ; GET DATA BYTE
            STR R4              ; STORE IN DISPLAY
L059D:      GHI RC              ; GET CONTROL BIT
            SHL                 ; SHIFT LEFT
            PHI RC              ; SAVE
            BDF L05AC           ; BRANCH IF DF=1
            LDN R4              ; GET DISPLAY BYTE
            SHR                 ; SHIFT RIGHT
            STR R4              ; STORE
            INC R4              ; INC DISPLAY LOCATION
            LDN R4              ; GET DISPLAY BYTE
            SHRC                ; SHIFT RIGHT W/CARRY
            STR R4              ; STORE
            DEC R4              ; RESET DISPLAY LOC.
            BR  L059D           ; BRANCH
L05AC:      GLO R4              ; GET DISPLAY ADDRESS
            ADI 0x08            ; MOVE DOWN ONE
            PLO R4              ; LINE
            DEC RC              ; DEC LINE COUNT
            GLO RC              ; GET LINE COUNT
            BNZ L0596           ; BRANCH NOT ZERO
            BR  L0580           ; BRANCH TO RETURN
L05B6:      LDI 0x00            ; SET TEMP. MEM. ADDR.
            PLO RF              ; TO ZERO
            BR  L0580           ; BRANCH TO RETURN

; ==================================================================
; Display Subroutine (interrupt handler; entered via hardware P=1/X=2
; at R1 = 0x04D0, per the "SET INTERRUPT COUNTER" setup in Main Program;
; 04CE/04CF (LDXA/RET) run once, from elsewhere, to enable interrupts)
; ==================================================================
            ORG 0x04CE
L04CE:      LDXA                ; INTERRUPT
            RET                 ; RETURN
L04D0:      DEC R2
            SAV
            DEC R2
            STR R2
            NOP                 ; NOPs FOR
            NOP
            NOP                 ; SYNC.
            LDI 0x06            ; SET DMA
            PHI R0              ; POINTER
            LDI 0x00
            PLO R0
L04DD:      GLO R0              ; INTERRUPT
            SEX R2              ; ROUTINE
            SEX R2
            DEC R0
            PLO R0
            SEX R2
            DEC R0
            PLO R0
            SEX R2
            DEC R0
            PLO R0
            BN1 L04DD           ; BR. TO INTERRUPT
            BR  L04CE           ; BRANCH TO RETURN
            DB  0x00            ; STACK AREA
            DB  0x00
            DB  0x00
            DB  0x00
            DB  0x00

; ==================================================================
; Sprite / data table (referenced by the Draw Subroutine via RF; each
; sprite is 5 bytes, matching "LDI 0x05 / PLO RC" line counts above)
; ==================================================================
            ORG 0x05F0
SPACESHIP:  DB 0x18, 0x3C, 0xFF, 0x24, 0x42
MAN:        DB 0x04, 0x1F, 0x04, 0x0A, 0x11
CRASH:      DB 0x24, 0x81, 0x24, 0x42, 0x18
            DS 1                ; TEMP. STORAGE (shown as "xx" -- uninitialized/scratch)
