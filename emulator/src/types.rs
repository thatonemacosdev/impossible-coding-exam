//! Fundamental primitive types and 24-bit word arithmetic for the Ω-Core architecture.

use std::fmt;

/// Maximum 24-bit unsigned integer value ($2^{24} - 1$).
pub const WORD24_MAX_U: u32 = 0x00FF_FFFF;
/// Modulus for 24-bit arithmetic ($2^{24}$).
pub const WORD24_MODULUS: u64 = 0x0100_0000;
/// Sign bit position in 24-bit representation (bit 23).
pub const WORD24_SIGN_BIT: u32 = 0x0080_0000;
/// Maximum signed 24-bit value ($+8,388,607$).
pub const WORD24_MAX_S: i32 = 8_388_607;
/// Minimum signed 24-bit value ($-8,388,608$).
pub const WORD24_MIN_S: i32 = -8_388_608;

/// A 24-bit word strictly bound within [0, 0x00FFFFFF].
#[derive(Copy, Clone, PartialEq, Eq, Default, Hash)]
pub struct Word24(pub u32);

impl Word24 {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);
    pub const MAX: Self = Self(WORD24_MAX_U);

    #[inline(always)]
    pub const fn from_u32(val: u32) -> Self {
        Self(val & WORD24_MAX_U)
    }

    #[inline(always)]
    pub const fn from_i32(val: i32) -> Self {
        Self((val as u32) & WORD24_MAX_U)
    }

    #[inline(always)]
    pub const fn to_u32(self) -> u32 {
        self.0
    }

    /// Converts 24-bit word to two's complement 32-bit signed integer.
    #[inline(always)]
    pub const fn to_i32(self) -> i32 {
        if (self.0 & WORD24_SIGN_BIT) != 0 {
            // Negative: sign extend bits 23..0 to 31..0
            (self.0 | !WORD24_MAX_U) as i32
        } else {
            self.0 as i32
        }
    }

    // Sub-word accessors
    #[inline(always)]
    pub const fn get_l(self) -> Self {
        Self(self.0 & 0x0000_0FFF)
    }

    #[inline(always)]
    pub const fn get_h(self) -> Self {
        Self((self.0 >> 12) & 0x0000_0FFF)
    }

    #[inline(always)]
    pub const fn get_b0(self) -> Self {
        Self(self.0 & 0x0000_00FF)
    }

    #[inline(always)]
    pub const fn get_b1(self) -> Self {
        Self((self.0 >> 8) & 0x0000_00FF)
    }

    #[inline(always)]
    pub const fn get_b2(self) -> Self {
        Self((self.0 >> 16) & 0x0000_00FF)
    }

    #[inline(always)]
    pub const fn with_l(self, val: Self) -> Self {
        Self((self.0 & 0x00FFF000) | (val.0 & 0x00000FFF))
    }

    #[inline(always)]
    pub const fn with_h(self, val: Self) -> Self {
        Self((self.0 & 0x00000FFF) | ((val.0 & 0x00000FFF) << 12))
    }

    #[inline(always)]
    pub const fn with_b0(self, val: Self) -> Self {
        Self((self.0 & 0x00FFFF00) | (val.0 & 0x000000FF))
    }

    #[inline(always)]
    pub const fn with_b1(self, val: Self) -> Self {
        Self((self.0 & 0x00FF00FF) | ((val.0 & 0x000000FF) << 8))
    }

    #[inline(always)]
    pub const fn with_b2(self, val: Self) -> Self {
        Self((self.0 & 0x0000FFFF) | ((val.0 & 0x000000FF) << 16))
    }

    // Arithmetic operations over Z / (2^24 Z)
    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn add(self, other: Self) -> (Self, bool) {
        let sum = self.0 + other.0;
        let word = Self(sum & WORD24_MAX_U);
        let carry = sum > WORD24_MAX_U;
        (word, carry)
    }

    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn sub(self, other: Self) -> (Self, bool) {
        let diff = (self.0 as i64) - (other.0 as i64);
        let word = Self((diff as u64 & (WORD24_MODULUS - 1)) as u32 & WORD24_MAX_U);
        let borrow = diff < 0;
        (word, borrow)
    }

    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn mul(self, other: Self) -> Self {
        let prod = (self.0 as u64) * (other.0 as u64);
        Self((prod & (WORD24_MODULUS - 1)) as u32)
    }

    #[inline(always)]
    pub fn mulh(self, other: Self) -> Self {
        let s1 = self.to_i32() as i64;
        let s2 = other.to_i32() as i64;
        let prod = s1 * s2;
        let high = prod >> 24;
        Self::from_i32(high as i32)
    }

    #[inline(always)]
    pub fn divs(self, other: Self) -> Result<Self, TrapReason> {
        let denom = other.to_i32();
        if denom == 0 {
            return Err(TrapReason::DivZero);
        }
        let numer = self.to_i32();
        let quot = numer / denom;
        Ok(Self::from_i32(quot))
    }

    #[inline(always)]
    pub fn mods(self, other: Self) -> Result<Self, TrapReason> {
        let denom = other.to_i32();
        if denom == 0 {
            return Err(TrapReason::DivZero);
        }
        let numer = self.to_i32();
        let rem = numer % denom;
        Ok(Self::from_i32(rem))
    }

    #[inline(always)]
    pub fn divu(self, other: Self) -> Result<Self, TrapReason> {
        if other.0 == 0 {
            return Err(TrapReason::DivZero);
        }
        Ok(Self(self.0 / other.0))
    }

    #[inline(always)]
    pub fn modu(self, other: Self) -> Result<Self, TrapReason> {
        if other.0 == 0 {
            return Err(TrapReason::DivZero);
        }
        Ok(Self(self.0 % other.0))
    }

    // Bitwise operations
    #[inline(always)]
    pub fn band(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[inline(always)]
    pub fn bor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline(always)]
    pub fn bxor(self, other: Self) -> Self {
        Self(self.0 ^ other.0)
    }

    #[inline(always)]
    pub fn bnot(self) -> Self {
        Self((!self.0) & WORD24_MAX_U)
    }

    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn shl(self, shift: Self) -> Self {
        let count = shift.0 % 24;
        Self((self.0 << count) & WORD24_MAX_U)
    }

    #[inline(always)]
    pub fn slr(self, shift: Self) -> Self {
        let count = shift.0 % 24;
        Self((self.0 >> count) & WORD24_MAX_U)
    }

    #[inline(always)]
    pub fn sar(self, shift: Self) -> Self {
        let count = shift.0 % 24;
        let signed = self.to_i32();
        let shifted = signed >> count;
        Self::from_i32(shifted)
    }

    #[inline(always)]
    pub fn rol(self, shift: Self) -> Self {
        let count = shift.0 % 24;
        if count == 0 {
            return self;
        }
        let v = self.0 & WORD24_MAX_U;
        let res = ((v << count) | (v >> (24 - count))) & WORD24_MAX_U;
        Self(res)
    }

    #[inline(always)]
    pub fn ror(self, shift: Self) -> Self {
        let count = shift.0 % 24;
        if count == 0 {
            return self;
        }
        let v = self.0 & WORD24_MAX_U;
        let res = ((v >> count) | (v << (24 - count))) & WORD24_MAX_U;
        Self(res)
    }

    /// Bit-reversal involution over 24 bits: bit i <-> bit (23 - i).
    #[inline(always)]
    pub fn rev(self) -> Self {
        let mut v = self.0;
        let mut r = 0u32;
        let mut i = 0;
        while i < 24 {
            r = (r << 1) | (v & 1);
            v >>= 1;
            i += 1;
        }
        Self(r & WORD24_MAX_U)
    }

    #[inline(always)]
    pub fn clz(self) -> Self {
        // Leading zeros relative to 24 bits
        if self.0 == 0 {
            Self(24)
        } else {
            let lz = self.0.leading_zeros() - 8;
            Self(lz)
        }
    }

    #[inline(always)]
    pub fn popcnt(self) -> Self {
        Self(self.0.count_ones())
    }
}

impl fmt::Debug for Word24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:06X}", self.0)
    }
}

impl fmt::Display for Word24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:06X}", self.0)
    }
}

/// Register index from r0 to r7.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Register {
    R0 = 0,
    R1 = 1,
    R2 = 2,
    R3 = 3,
    R4 = 4,
    R5 = 5,
    R6 = 6,
    R7 = 7,
}

impl Register {
    pub fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(Self::R0),
            1 => Some(Self::R1),
            2 => Some(Self::R2),
            3 => Some(Self::R3),
            4 => Some(Self::R4),
            5 => Some(Self::R5),
            6 => Some(Self::R6),
            7 => Some(Self::R7),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Sub-word register slice selector.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SubSlice {
    Full, // Full 24 bits
    L,    // Low 12 bits [11:0]
    H,    // High 12 bits [23:12]
    B0,   // Byte 0 [7:0]
    B1,   // Byte 1 [15:8]
    B2,   // Byte 2 [23:16]
}

/// Retention policy for register reading (Quantum Decay vs Retention Sigil).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RetentionPolicy {
    /// Destructive read: Register or subslice is cleared to 0 after observation.
    Consume,
    /// Preserving read: Register value retained intact (prefix `@`).
    Retain,
}

/// A destination register operand with an optional sub-slice.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RegDest {
    pub reg: Register,
    pub slice: SubSlice,
}

/// A source register operand with sub-slice and retention policy.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RegSrc {
    pub reg: Register,
    pub slice: SubSlice,
    pub retention: RetentionPolicy,
}

/// Source operand: either a register (with retention rule) or an immediate literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceOperand {
    Reg(RegSrc),
    Imm(Word24),
}

/// Conditional branch predicate codes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BranchCond {
    Eq,  // equal
    Ne,  // not equal
    Lt,  // signed less than
    Le,  // signed less than or equal
    Gt,  // signed greater than
    Ge,  // signed greater than or equal
    Ltu, // unsigned less than
    Leu, // unsigned less than or equal
    Gtu, // unsigned greater than
    Geu, // unsigned greater than or equal
}

/// Processor trap and termination reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrapReason {
    Halt(u32),
    DivZero,
    AddrOutOfBounds(u32),
    IllegalInstruction(String),
    MaxCyclesExceeded(u64),
}

impl fmt::Display for TrapReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Halt(code) => write!(f, "HALT(code=0x{:06X})", code),
            Self::DivZero => write!(f, "TRAP_DIV_ZERO"),
            Self::AddrOutOfBounds(addr) => write!(f, "TRAP_ADDR_OUT_OF_BOUNDS(0x{:06X})", addr),
            Self::IllegalInstruction(msg) => write!(f, "TRAP_ILLEGAL_INST({})", msg),
            Self::MaxCyclesExceeded(limit) => write!(f, "TRAP_MAX_CYCLES_EXCEEDED({})", limit),
        }
    }
}
