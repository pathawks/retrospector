// References:
//   Intel 8048 instruction set:
//     https://datasheetspdf.com/pdf/509798/Intel/8048/1
//   8048 opcode encoding:
//     https://en.wikipedia.org/wiki/Intel_MCS-48#Instructions

/// The Intel 8048 JMP instruction has the bit pattern `xxx00100` in the opcode
/// byte, where `xxx` encodes bits [10:8] of the 11-bit destination address.
pub(super) const JMP_MASK: u8 = 0x1F;
pub(super) const JMP_OPCODE: u8 = 0x04;
pub(super) const CALL_OPCODE: u8 = 0x14;

/// Decode the target address from an 8048 JMP instruction.
/// The opcode's upper 3 bits carry addr[10:8]; the next byte is addr[7:0].
/// Bit 11 is inherited from the current PC and is always 0 for cartridge code
/// in the lower 2 KB bank.
#[allow(clippy::arithmetic_side_effects)]
pub(super) fn decode_jmp(opcode: u8, operand: u8) -> u16 {
    (((opcode & 0xE0) as u16) << 3) | (operand as u16)
}

pub(super) fn is_jmp(b: u8) -> bool {
    b & JMP_MASK == JMP_OPCODE
}

pub(super) fn is_call(b: u8) -> bool {
    b & JMP_MASK == CALL_OPCODE
}

/// Return the byte-length of an Intel 8048 instruction.
pub(super) fn instruction_len(op: u8) -> usize {
    // JMP / CALL
    if is_jmp(op) || is_call(op) {
        return 2;
    }
    // Immediate ALU: ADD/ORL/ANL/XRL A, #imm  —  MOV A, #imm
    if matches!(op, 0x03 | 0x23 | 0x43 | 0x53 | 0xD3) {
        return 2;
    }
    // MOV Rn, #imm (0xB8–0xBF)  /  MOV @Rn, #imm (0xB0, 0xB1)
    if (0xB0..=0xBF).contains(&op) {
        return 2;
    }
    // Conditional jumps (JZ, JNZ, JC, JNC, JBn, JTn, JNTn, JF0, JF1, JNI, JTF)
    if matches!(
        op,
        0x12 | 0x32
            | 0x52
            | 0x72
            | 0x92
            | 0xB2
            | 0xD2
            | 0xF2
            | 0x96
            | 0xC6
            | 0xF6
            | 0xE6
            | 0x76
            | 0x86
            | 0x36
            | 0x26
            | 0x56
            | 0x46
            | 0x16
            | 0xB6
    ) {
        return 2;
    }
    // DJNZ Rn (0xE8–0xEF)
    if (0xE8..=0xEF).contains(&op) {
        return 2;
    }
    // Everything else is 1 byte
    1
}

/// Classify what an instruction does to the accumulator.
#[derive(PartialEq)]
pub(super) enum AccEffect {
    /// Instruction reads/tests the current value of A.
    Reads,
    /// Instruction overwrites A without reading its current value.
    Overwrites,
    /// Instruction does not affect A.
    Neutral,
}

pub(super) fn acc_effect(op: u8) -> AccEffect {
    // Instructions that READ / USE the current value of A:
    if matches!(
        op,
        0x07 |                       // DEC A
        0x17 |                       // INC A
        0x37 |                       // CPL A
        0x47 | 0x67 | 0x77 |        // SWAP A / RRC A / RR A
        0xE7 | 0xF7 |               // RL A / RLC A
        0xA0 | 0xA1 |               // MOV @R0, A / MOV @R1, A
        0xA8
            ..=0xAF |               // MOV Rn, A
        0xC6 | 0x96 |               // JZ / JNZ
        0xB3 | 0xA3 |               // JMPP @A / MOVP A, @A
        0x02 | 0x39 | 0x3A |        // OUTL BUS/P1/P2, A
        0x62 |                       // MOV T, A
        0x57 // DA A
    ) {
        return AccEffect::Reads;
    }
    // Immediate ALU ops also read A: ADD/ORL/ANL/XRL A, #imm
    if matches!(op, 0x03 | 0x43 | 0x53 | 0xD3) {
        return AccEffect::Reads;
    }
    // ADD/ADDC/ANL/ORL/XRL A, Rn also read A
    if matches!(
        op & 0xF8,
        0x60 |  // ADD A, Rn  (0x68-0x6F)
        0x70 |  // ADDC A, Rn (0x78-0x7F)
        0x58 |  // ANL A, Rn  (0x58-0x5F)
        0x48 |  // ORL A, Rn  (0x48-0x4F)
        0xD8 // XRL A, Rn  (0xD8-0xDF)
    ) {
        return AccEffect::Reads;
    }
    // XCH A, Rn / XCH A, @Rn — both reads and writes A
    if (0x28..=0x2F).contains(&op) || op == 0x20 || op == 0x21 {
        return AccEffect::Reads;
    }

    // Instructions that OVERWRITE A (the key code is lost):
    if matches!(
        op,
        0x23 |                       // MOV A, #imm
        0x27 |                       // CLR A
        0x08 | 0x09 | 0x0A |        // INS A, BUS / IN A, P1 / IN A, P2
        0xF0 | 0xF1 |               // MOV A, @R0 / MOV A, @R1
        0x42 | 0xC7 // MOV A, T / MOV A, PSW
    ) {
        return AccEffect::Overwrites;
    }
    // MOV A, Rn (0xF8–0xFF)
    if (0xF8..=0xFF).contains(&op) {
        return AccEffect::Overwrites;
    }
    // JMP / CALL — control leaves; treat as overwriting A since the key code
    // is not being tested here.
    if is_jmp(op) || is_call(op) {
        return AccEffect::Overwrites;
    }

    AccEffect::Neutral
}
