STACK    EQU 0x3FFF
;
        ORG 0x0000
;
        GHI R0;                 R0.1:R3.1:R4.1:R5.1
        PHI R3
        PHI R4
        PHI R5
        LDI LOW START;          START:R3.0
        PLO R3
        SEP R3;                 R3:P
START:  LDI HIGH STACK;         STACK:R2
        PHI R2
        LDI LOW STACK
        PLO R2                   ; set up R2 = stack pointer
        LDI LOW SCRTCALL;       SCRTCALL:R4.0
        PLO R4                   ; set up R4 -> SCRT call handler
        LDI LOW SCRTRET;        SCRTRET:R5.0
        PLO R5                   ; set up R5 -> SCRT return handler
;
; ---- Main loop
;
L0014:  CALL OUTCR;             Print CR
        CALL L0067;             Print > (prompt)
        CALL GETCH
        XRI 0x44;               Is is 'D'?
        LBZ L00EB;              /DUMP
;
        XRI 0x01;               'E'?
        LBZ L0140
;
        XRI 0x03
        LBZ L0178
;
        XRI 0x01
        LBZ L018F
;
        XRI 0x1F
        LBZ L01A0
;
L0036:  CALL L006D;             Print * and loop
        BR L0014
;
; ---- SCRT "CALL"
;
CALLE:  SEP R3
SCRTCALL: PHI RF
        SEX R2;
        GHI R6
        STXD
        GLO R6
        STXD
        GHI R3
        PHI R6
        GLO R3
        PLO R6
        LDA R6
        PHI R3
        LDA R6
        PLO R3
        GHI RF
        BR CALLE
;
; ---- SCRT "RETN"
;
RETNE:  SEP R3
SCRTRET: PHI RF
        SEX R2
        GHI R6
        PHI R3
        GLO R6
        PLO R3
        INC R2
        LDXA
        PLO R6
        LDX
        PHI R6
        GHI RF
        BR RETNE;
;
; ---- OUTPUT a Carriage Return
OUTCR:  LDI 0x0D;               CR
        CALL L01AD
;
; ---- Output an LF
;
        LDI 0x0A                LF
        LSKP
;
; ---- Output a Colon
;
L0064:  LDI 0x3A                COLON
        LSKP
;
; ---- Output a GE sign
;
L0067:  LDI 0x3E                >
        LSKP
;
; ---- Output a Space
;
L006A:  LDI 0x20;               SPACE
        LSKP
;
; ---- Output an asterisk
;
L006D:  LDI 0x2A                *
        PHI RF
        LBR L01AD
;
; ---- Read hex byte from keyboard
;
L0073:  CALL GETCH;             First nibble
        CALL L0089
        SHL
        SHL
        SHL
        SHL
        PLO RF;                 into RF.0
        CALL GETCH;
        CALL L0089;
        STR R2;                 D:(R2)
        GLO RF
        OR                      (R2) OR D into D
        PHI RF
        RETN
;
; ---- ASCII char to hex nibble in D
;
L0089:  SMI 0x41;               D - 'A'
        BDF LETTER;
        ADI 0x11;               Add 0x11 for numbers
        LSKP
LETTER: SMI 0x06;               minus another 6 for letters
        ANI 0x0F                take only lower 4 bits
        RETN
;
; ---- Hex byte to ASCII and Print
;
L0095:  CALL L009B;             One hex digit in D
        GLO RF
        BR L00A0
;
; ---- Hex high nibble to ASCII char and Print
;
L009B:  PLO RF;                 D:RF.0
        SHR                     high nibble to low
        SHR
        SHR
        SHR
L00A0:  ANI 0x0F;               Discard all but lower 4 bits
        ADI 0xF6
        LSNF
        ADI 0x07
        SMI 0xC6
        PHI RF
        LBR L01AD
;
; ---- Read 16-bit hex number from KB into RD
;
L00AD:  CALL L0073
        PHI RD
        CALL L0073
        PLO RD
        RETN
;
; ---- Read start,length from KB into R8,R9
;
L00B6:  CALL L00AD
        GHI RD;                 RD:R8
        PHI R8
        GLO RD
        PLO R8
        CALL GETCH
        XRI 0x2C;               Is comma?
        BNZ L0036
        CALL L00AD
        GHI RD;                 RD:R9
        PHI R9
        GLO RD
        PLO R9
        RETN
;
; ---- /MLOOP if R9 == R8
;
L00CC:  GHI R9
        STR R2
        GHI R8
        XOR
        BNZ L00DB
        GLO R9
        STR R2
        GLO R8
        XOR
        BNZ L00DB
        LBR L0014;              /MLOOP
L00DB:  RETN
;
; ---- Print address followed by ": "
;
L00DC:  GHI R8
        CALL L0095
        GLO R8
        CALL L0095
        CALL L0064;             Print Colon
        CALL L006A;             Print space
        RETN
;
; ---- Dump handler
;
L00EB:  CALL L00B6;             Read start,length into R8,R9
        CALL OUTCR;             Print CR
L00F1:  CALL OUTCR               another CR
;
        GHI R8;                 R8:R7 save address for ASCII
        PHI R7
        GLO R8
        PLO R7
        CALL L00DC;             Print address ": "
L00FB:  LDN R8;                 (R8):D and
        CALL L0095;              print D as ASCII-hex
        CALL L00CC;             R8=R9? /MLOOP
        GLO R8
        INC R8
        ANI 0x0F
        XRI 0x0F
        BZ L0113;               16 bytes yet?
        SHR
        BDF L0110
        CALL L006A;             Print space
L0110:  LBR L00FB
L0113:  CALL L006A;             Print space
        LDI 0x28;               Print (
        CALL L01AD
L011B:  LDN R7;
        STR R2
        LDI 0x20;               Check below space
        SD
        BNF L0127
        LDI 0x7F                Or above 127
        SD
        BNF L012A
L0127:  LDI 0x2E;               Assume we'll print a '.'
        SKP
L012A:  LDN R7;                 (R7):D
        CALL L01AD              Print D
        GLO R7
        INC R7
        ANI 0x0F
        XRI 0x0F
        BZ L0138;               Done all 16 yet?
        BR L011B                 no
L0138:  LDI 0x29;                yes, print )
        CALL L01AD
        LBR L00F1;              Continue dump.
;
; ---- Edit handler
;
L0140:  CALL L00AD;             Read 16-bit hex number into RD
        GHI RD;                 RD:R8
        PHI R8
        GLO RD
        PLO R8
L0147:  CALL GETCH
        XRI 0x0D;               CR?
        LBZ L0014
;
        XRI 0x21
        BZ L0161
;
        XRI 0x0C
        BZ L015A
        LBR L0036;              Print * and /MLOOP
;
L015A:  LDN R8
        CALL L0095
        CALL L0064;             Print Colon
L0161:  CALL L0073;             Read byte
        STR R8
        GLO R8
        INC R8
        ANI 0x0F
        XRI 0x0F
        BZ L0173
        XRI 0x08
        BZ L0173
        BR L0147
L0173:  CALL OUTCR
        BR L0147
;
; ----
;
L0178:  CALL L00B6
        CALL GETCH
        XRI 0x2C
        LBNZ L0036
        CALL L0073
        PLO RF
L0187:  GLO RF
        STR R8
        CALL L00CC
        INC R8
        BR L0187
;
; ----
;
L018F:  CALL L006A
        CALL L00AD
        GHI R3
        PHI R9
        LDI 0x9B
        PLO R9
        SEP R9
        GHI RD
        PHI R3
        GLO RD
        PLO R3
        SEP R3
L01A0:  LBR L0014
;
; ---- KB char to D and RF.1
;
GETCH:  BN2 GETCH;              Wait for Keypress
        INP 5;                  Get keyed character
        PHI RF;                  into D and RF.1
        BR L01AD;                and print it (echo it.)
;
        DB 0x8C,0x73,0x8F,0x73
;
; ---- Print char in RF.1
;
L01AD:  GHI RF;                 Char to printer
        STR R2;                 Store for OUT inst
        LDI 0x0A;               Skip if LF
        XOR
        BZ L01B8
L01B4:  BN1 L01B4;              Wait printer ready
        OUT 3                   Output the byte at (R2)
        DEC R2;                 OUT bumps Rx=2 so fix it
L01B8:  GHI RF;                 Restore char to D
        RETN
;
        DB 0xF8,0xDD,0xAC,0xF8,0x09,0xAF,0x7B,0xDC
        DB 0x28,0x9F,0x76,0xBF,0x33,0xCA,0x7B,0x38
        DB 0x7A,0x2F,0x8F,0x3A,0xC1,0x7A,0xDC,0x50
        DB 0x72,0xBF,0x72,0xAF,0x72,0xAC,0xF0,0xBC
        DB 0x9F,0xD5,0xD3,0x43,0xC4,0xC4,0xC4,0xC4
        DB 0xC4,0xFF,0x01,0x3A,0xDE,0x30,0xDC
