; Bit-banged serial I/O for the Giant Board / VID-1 terminal. SEROUT, SERIN
; and the shared bit-delay are extracted instruction-for-instruction (only
; register names changed) from the Netronics Tiny BASIC recreation
; (sample-code/asm/Tinybasi.asm: OUTPUTR, INPUTR, BRKTST and the Z296/Z158
; delay loop), which is proven working against this emulator's
; BitBangSerial model. Q is the transmit line; EF3 is the receive line
; (this emulator's defaults: tx_inverted=false, rx_ef=3, rx_inverted=false).
;
; Tiny BASIC measures its delay constant from the user's first keypress
; (autobaud); that measurement code is tangled up with its own boot
; sequence and isn't reusable as a clean subroutine. Since this emulator's
; baud rate is a fixed, known setting (not unknown real hardware), this
; file uses a plain calibrated constant instead: BSBAUD, 0x3C by default.
; This is NOT Tiny BASIC's own "TIME DB 0x9A" byte (that turned out to be
; an unrelated blink-timer default, not a baud constant); 0x3C was found
; empirically against this emulator's default 300-baud BitBangSerial model
; (clock_hz/8 cycles per machine cycle): clean round-trips of SEROUT and
; SERIN were verified for every even value from 0x38 to 0x3E, so 0x3C sits
; in the middle of that margin. Re-derive it (see the asm1802 crate's
; scratch calibration harness) if the bit-period instructions above ever
; change, or for a different baud rate.
;
; SERIN only reliably receives 7-bit values (bit 7 always reads back 0),
; inherited unchanged from Tiny BASIC's INPUTR: its receive loop's last
; data bit is computed but the loop exits before storing it back. This is
; invisible for real ASCII keyboard input (bit 7 is always 0 on the wire;
; `Machine::term_key` itself masks with 0x7F), which is the only use this
; file is meant for.
;
; Registers (override by EQUing these before INCLUDE; the defaults match
; Tiny BASIC's own code, so INCLUDEing this unchanged in place of Tiny
; BASIC's copy is a no-op):
;   BSTIME   EQU RE   timing state (BSTIME.1 = the delay constant); must
;                     stay free for the whole session once initialised
;   BSOUT    EQU RC   scratch for SEROUT's byte/shift register
;   BSIN     EQU RF   scratch for SERIN's byte/shift register (also used
;                     by SEROUT as its bit counter)
;   BSBAUD   EQU 0x3C the delay constant (bigger = slower); override for a
;                     baud rate other than this emulator's 300 default
;
; Entry points (this file does not define CALL/RETURN; it uses whatever
; they already mean in the including program, the SEP R4/SEP R5 technique
; every other sample program here sets up):
;   BSINIT   call once before the first SEROUT/SERIN; sets BSTIME from BSBAUD
;   SEROUT   byte to send in D; sends it; clobbers D, DF; returns via RETURN
;   SERIN    waits for a byte; returns it in D; clobbers DF; returns via RETURN
;   SERBRK   break test: DF = 1 if EF3 is asserted (break held)
;
            IFNDEF BSTIME
BSTIME      EQU     RE
            ENDIF
            IFNDEF BSOUT
BSOUT       EQU     RC
            ENDIF
            IFNDEF BSIN
BSIN        EQU     RF
            ENDIF
            IFNDEF BSBAUD
BSBAUD      EQU     0x3C
            ENDIF
;
BSINIT      LDI     BSBAUD
            PHI     BSTIME
            RETURN
;
; ---------------------------------------------------------- SERBRK (break)
;
SERBRK      ADI     0           BREAK TEST: DF = EF3 asserted
            GHI     BSTIME
            BNZ     BSBRKC
            BN3     BSBRKR
            LSKP
BSBRKC      BN3     BSBRKR
            SMI     0
;
; ------------------------------------------------------ shared bit-delay
;
; BSDELAY waits one bit period, using BSTIME.1. BSDWAIT is the same loop
; entered with the count already in D (SERIN uses this to wait half a bit).
;
BSDELAY     GHI     BSTIME
            ANI     0xFE
BSDWAIT     PLO     BSTIME
            LSZ
            DEC     BSTIME
            GLO     BSTIME
            BNZ     BSDWAIT
BSBRKR      RETURN
;
; -------------------------------------------------------------- SERIN
;
SERIN       GHI     BSTIME      ; BSTIME.1 is never 0 (BSINIT sets it from
            BZ      BSBRKR          BSBAUD); this guards against calling
                                    ; SERIN before BSINIT
BSRXWAIT    BN3     BSRXWAIT    ; wait for the start bit (EF3 asserted)
            SHR                 ; half a bit, to land mid-bit
            CALL    BSDWAIT
            BN3     SERIN       ; glitch: re-synchronise
            LDI     0x7F
BSRXBIT     PLO     BSIN
            GHI     BSTIME
            SHR
            B3      BSRXQ
            BNF     BSRXM
            SEQ
            SKP
BSRXQ       REQ
BSRXM       CALL    BSDELAY
            GLO     BSIN
            SHR
            PLO     BSIN
            B3      BSRXSTOP
            ORI     0x80
BSRXSTOP    BDF     BSRXBIT
            REQ
            CALL    BSDELAY
            GLO     BSIN
            RETURN
;
; ------------------------------------------------------------- SEROUT
;
SEROUT      PLO     BSOUT
            PHI     BSOUT
            GHI     BSTIME      ; see SERIN: BSTIME.1 is never 0 after BSINIT
            BNZ     BSTXSER
            GLO     BSOUT
            RETURN
BSTXSER     LDI     0x0A
            PLO     BSIN
            CALL    BSDELAY
            ADI     0
BSTXBIT     CALL    BSDELAY
            LSNF
            REQ
            SKP
            SEQ
            GHI     BSOUT
            SMI     0
            SHRC
            PHI     BSOUT
            DEC     BSIN
            GLO     BSIN
            BNZ     BSTXBIT
            GLO     BSOUT
            RETURN
