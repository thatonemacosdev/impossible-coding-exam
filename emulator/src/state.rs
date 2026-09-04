//! Execution state machine (σ) for the Ω-Core virtual machine.

use crate::bank::BankTracker;
use crate::metrics::Metrics;
use crate::types::{
    RegDest, RegSrc, RetentionPolicy, SubSlice, TrapReason, Word24,
};

pub const MEMORY_SIZE: usize = 65536;
pub const RING_BUFFER_CAPACITY: usize = 8;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub z: bool, // Zero flag
    pub s: bool, // Sign flag
    pub v: bool, // Overflow flag
    pub c: bool, // Carry flag
}

pub struct State {
    /// 8 general-purpose 24-bit registers (r0..r7)
    pub r: [Word24; 8],
    /// Hardware return-address ring buffer of fixed capacity 8
    pub rb: [Word24; RING_BUFFER_CAPACITY],
    /// Ring buffer head pointer (0..7)
    pub rp: usize,
    /// Active return-address frames currently tracked (0..8)
    pub call_depth: usize,
    /// Word-addressed 64K bare-metal memory (zero heap allocations during run)
    pub mem: Box<[Word24; MEMORY_SIZE]>,
    /// Program counter
    pub pc: usize,
    /// Asymmetric memory bank controller
    pub banks: BankTracker,
    /// Processor status flags
    pub flags: Flags,
    /// Execution termination status (None = running, Some = halted/trapped)
    pub halted: Option<TrapReason>,
    /// Hardware performance and cycle metrics
    pub metrics: Metrics,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("r", &self.r)
            .field("pc", &self.pc)
            .field("rp", &self.rp)
            .field("call_depth", &self.call_depth)
            .field("flags", &self.flags)
            .field("halted", &self.halted)
            .field("cycles", &self.metrics.total_cycles)
            .finish()
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        // Pre-allocate memory array once at creation time
        let mem = vec![Word24::ZERO; MEMORY_SIZE]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| panic!("Failed to allocate 64K memory array"));

        Self {
            r: [Word24::ZERO; 8],
            rb: [Word24::ZERO; RING_BUFFER_CAPACITY],
            rp: 0,
            call_depth: 0,
            mem,
            pc: 0,
            banks: BankTracker::new(),
            flags: Flags::default(),
            halted: None,
            metrics: Metrics::new(),
        }
    }

    /// Resets processor registers, PC, banks, and ring buffer while leaving memory intact.
    pub fn reset_registers(&mut self) {
        self.r = [Word24::ZERO; 8];
        self.rb = [Word24::ZERO; RING_BUFFER_CAPACITY];
        self.rp = 0;
        self.call_depth = 0;
        self.pc = 0;
        self.banks.reset();
        self.flags = Flags::default();
        self.halted = None;
        self.metrics = Metrics::new();
    }

    /// Evaluates a source register operand, applying destructive quantum read decay
    /// unless explicitly guarded by the RetentionPolicy::Retain sigil (@).
    pub fn eval_reg_src(&mut self, src: &RegSrc) -> Word24 {
        let reg_idx = src.reg.index();
        let current_val = self.r[reg_idx];

        let value = match src.slice {
            SubSlice::Full => current_val,
            SubSlice::L => current_val.get_l(),
            SubSlice::H => current_val.get_h(),
            SubSlice::B0 => current_val.get_b0(),
            SubSlice::B1 => current_val.get_b1(),
            SubSlice::B2 => current_val.get_b2(),
        };

        // If destructive consumption is active, immediately clear the read slice to 0.
        if src.retention == RetentionPolicy::Consume {
            self.r[reg_idx] = match src.slice {
                SubSlice::Full => Word24::ZERO,
                SubSlice::L => current_val.with_l(Word24::ZERO),
                SubSlice::H => current_val.with_h(Word24::ZERO),
                SubSlice::B0 => current_val.with_b0(Word24::ZERO),
                SubSlice::B1 => current_val.with_b1(Word24::ZERO),
                SubSlice::B2 => current_val.with_b2(Word24::ZERO),
            };
        }

        value
    }

    /// Writes a value to the destination register with sub-slice masking.
    pub fn write_reg_dest(&mut self, dest: &RegDest, val: Word24) {
        let reg_idx = dest.reg.index();
        let current_val = self.r[reg_idx];

        self.r[reg_idx] = match dest.slice {
            SubSlice::Full => val,
            SubSlice::L => current_val.with_l(val),
            SubSlice::H => current_val.with_h(val),
            SubSlice::B0 => current_val.with_b0(val),
            SubSlice::B1 => current_val.with_b1(val),
            SubSlice::B2 => current_val.with_b2(val),
        };
    }

    /// Pushes a return address to the hardware circular ring buffer.
    /// Silently wraps if recursion exceeds capacity D = 8.
    pub fn push_ring(&mut self, return_addr: Word24) {
        self.rp = (self.rp + 1) % RING_BUFFER_CAPACITY;
        self.rb[self.rp] = return_addr;
        if self.call_depth < RING_BUFFER_CAPACITY {
            self.call_depth += 1;
        }
    }

    /// Pops the top return address from the hardware circular ring buffer.
    pub fn pop_ring(&mut self) -> Word24 {
        let addr = self.rb[self.rp];
        self.rp = (self.rp + RING_BUFFER_CAPACITY - 1) % RING_BUFFER_CAPACITY;
        if self.call_depth > 0 {
            self.call_depth -= 1;
        }
        addr
    }

    /// Updates ALU flags (Zero, Sign).
    pub fn update_flags_result(&mut self, val: Word24) {
        self.flags.z = val.0 == 0;
        self.flags.s = (val.0 & crate::types::WORD24_SIGN_BIT) != 0;
    }
}
