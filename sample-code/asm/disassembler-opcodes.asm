;
; TITLE('DISASSEMBLER VER 3.0 OPCODE TABLE') 12/20/81.
;*----------------------------------------------------------------------*
;
; O P C O D E   T A B L E
; ========================
; THIS IS A MORE COMPACT VERSION OF THE OPCODE
; TABLE CURRENTLY EMPLOYED BY THE VERSION 1.0
; DISASSEMBLER.
;
; EACH CODE REQUIRES 6 AS OPPOSED TO 8 BYTES
; FOR IT'S DEFINITION.  THE BYTES HAVE THE FOLLOWING
; SIGNIFIGANCE:
;
;    BYTE 1 = HEX OPERATION CODE TO MATCH
;    BYTE 2 = CODE FOR TYPE OF OPERAND REQUIRED
;             00 = NO OPERAND
;             01 = ONE BYTE OPERAND
;             02 = TWO BYTE OPERAND
;             04 = REGISTER OPERAND
;    BYTE 3-6 = ASCII REPRESENTATION OF MNEMONIC
;               FOR THIS INSTRUCTION, WRITTEN BELOW AS
;               DB 'MMMM' (4 CHARACTERS, SPACE-PADDED)
;
        DW #0000  ; op 00, no operand
        DB 'IDL '
        DW #0104  ; op 01, register operand
        DB 'LDN '
        DW #1004  ; op 10, register operand
        DB 'INC '
        DW #2004  ; op 20, register operand
        DB 'DEC '
        DW #3001  ; op 30, one byte operand
        DB 'BR  '
        DW #3101  ; op 31, one byte operand
        DB 'BQ  '
        DW #3201  ; op 32, one byte operand
        DB 'BZ  '
        DW #3301  ; op 33, one byte operand
        DB 'BDF '
        DW #3401  ; op 34, one byte operand
        DB 'B1  '
        DW #3501  ; op 35, one byte operand
        DB 'B2  '
        DW #3601  ; op 36, one byte operand
        DB 'B3  '
        DW #3701  ; op 37, one byte operand
        DB 'B4  '
        DW #3800  ; op 38, no operand
        DB 'SKP '
        DW #3901  ; op 39, one byte operand
        DB 'BNQ '
        DW #3A01  ; op 3A, one byte operand
        DB 'BNZ '
        DW #3B01  ; op 3B, one byte operand
        DB 'BNF '
        DW #3C01  ; op 3C, one byte operand
        DB 'BN1 '
        DW #3D01  ; op 3D, one byte operand
        DB 'BN2 '
        DW #3E01  ; op 3E, one byte operand
        DB 'BN3 '
        DW #3F01  ; op 3F, one byte operand
        DB 'BN4 '
        DW #4004  ; op 40, register operand
        DB 'LDA '
        DW #5004  ; op 50, register operand
        DB 'STR '
        DW #6000  ; op 60, no operand
        DB 'IRX '
        DW #6104  ; op 61, register operand
        DB 'OUT '
        DW #6800  ; op 68, no operand (unimplemented on the 1802)
        DB 'DATA'
        DW #6904  ; op 69, register operand
        DB 'INP '
        DW #7000  ; op 70, no operand
        DB 'RET '
        DW #7100  ; op 71, no operand
        DB 'DIS '
        DW #7200  ; op 72, no operand
        DB 'LDXA'
        DW #7300  ; op 73, no operand
        DB 'STXD'
        DW #7400  ; op 74, no operand
        DB 'ADC '
        DW #7500  ; op 75, no operand
        DB 'SDB '
        DW #7600  ; op 76, no operand
        DB 'SHRC'
        DW #7700  ; op 77, no operand
        DB 'SMB '
        DW #7800  ; op 78, no operand
        DB 'SAV '
        DW #7900  ; op 79, no operand
        DB 'MARK'
        DW #7A00  ; op 7A, no operand
        DB 'REQ '
        DW #7B00  ; op 7B, no operand
        DB 'SEQ '
        DW #7C01  ; op 7C, one byte operand
        DB 'ADCI'
        DW #7D01  ; op 7D, one byte operand
        DB 'SDBI'
        DW #7E00  ; op 7E, no operand
        DB 'SHLC'
        DW #7F01  ; op 7F, one byte operand
        DB 'SMBI'
        DW #8004  ; op 80, register operand
        DB 'GLO '
        DW #9004  ; op 90, register operand
        DB 'GHI '
        DW #A004  ; op A0, register operand
        DB 'PLO '
        DW #B004  ; op B0, register operand
        DB 'PHI '
        DW #C002  ; op C0, two byte operand
        DB 'LBR '
        DW #C102  ; op C1, two byte operand
        DB 'LBQ '
        DW #C202  ; op C2, two byte operand
        DB 'LBZ '
        DW #C302  ; op C3, two byte operand
        DB 'LBDF'
        DW #C400  ; op C4, no operand
        DB 'NOP '
        DW #C500  ; op C5, no operand
        DB 'LSNQ'
        DW #C600  ; op C6, no operand
        DB 'LSNZ'
        DW #C700  ; op C7, no operand
        DB 'LSNF'
        DW #C800  ; op C8, no operand
        DB 'LSKP'
        DW #C902  ; op C9, two byte operand
        DB 'LBNQ'
        DW #CA02  ; op CA, two byte operand
        DB 'LBNZ'
        DW #CB02  ; op CB, two byte operand
        DB 'LBNF'
        DW #CC00  ; op CC, no operand
        DB 'LSIE'
        DW #CD00  ; op CD, no operand
        DB 'LSQ '
        DW #CE00  ; op CE, no operand
        DB 'LSZ '
        DW #CF00  ; op CF, no operand
        DB 'LSDF'
        DW #D004  ; op D0, register operand
        DB 'SEP '
        DW #D402  ; op D4, two byte operand (SCRT CALL)
        DB 'CALL'
        DW #D500  ; op D5, no operand (SCRT RETURN)
        DB 'RETN'
        DW #D604  ; op D6, register operand
        DB 'SEP '
        DW #E004  ; op E0, register operand
        DB 'SEX '
        DW #F000  ; op F0, no operand
        DB 'LDX '
        DW #F100  ; op F1, no operand
        DB 'OR  '
        DW #F200  ; op F2, no operand
        DB 'AND '
        DW #F300  ; op F3, no operand
        DB 'XOR '
        DW #F400  ; op F4, no operand
        DB 'ADD '
        DW #F500  ; op F5, no operand
        DB 'SD  '
        DW #F600  ; op F6, no operand
        DB 'SHR '
        DW #F700  ; op F7, no operand
        DB 'SM  '
        DW #F801  ; op F8, one byte operand
        DB 'LDI '
        DW #F901  ; op F9, one byte operand
        DB 'ORI '
        DW #FA01  ; op FA, one byte operand
        DB 'ANI '
        DW #FB01  ; op FB, one byte operand
        DB 'XRI '
        DW #FC01  ; op FC, one byte operand
        DB 'ADI '
        DW #FD01  ; op FD, one byte operand
        DB 'SDI '
        DW #FE00  ; op FE, no operand
        DB 'SHL '
        DW #FF01  ; op FF, one byte operand
        DB 'SMI '
