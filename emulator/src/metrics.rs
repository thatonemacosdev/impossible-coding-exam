//! Cycle-accurate metrics and hardware event collector.

use crate::bank::NUM_BANKS;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metrics {
    pub total_cycles: u64,
    pub instructions_executed: u64,
    pub alu_instructions: u64,
    pub mem_instructions: u64,
    pub branch_instructions: u64,
    pub branches_taken: u64,
    pub call_instructions: u64,
    pub ret_instructions: u64,
    pub bank_stall_cycles: u64,
    pub branch_mispredict_stall_cycles: u64,
    pub memory_high_water_mark: u32,
    pub peak_ring_depth: usize,
    pub bank_access_counts: [u64; NUM_BANKS],
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_alu(&mut self, cost_cycles: u64) {
        self.instructions_executed += 1;
        self.alu_instructions += 1;
        self.total_cycles += cost_cycles;
    }

    pub fn record_mem(&mut self, addr: u32, stall: u64, total_latency: u64) {
        self.instructions_executed += 1;
        self.mem_instructions += 1;
        self.total_cycles += total_latency;
        self.bank_stall_cycles += stall;
        let bank = (addr as usize) % NUM_BANKS;
        self.bank_access_counts[bank] += 1;
        if addr > self.memory_high_water_mark {
            self.memory_high_water_mark = addr;
        }
    }

    pub fn record_branch(&mut self, taken: bool) {
        self.instructions_executed += 1;
        self.branch_instructions += 1;
        if taken {
            self.branches_taken += 1;
            // 1 base cycle + 2 mispredict penalty = 3 cycles
            self.total_cycles += 3;
            self.branch_mispredict_stall_cycles += 2;
        } else {
            // 1 base cycle
            self.total_cycles += 1;
        }
    }

    pub fn record_call(&mut self, current_depth: usize) {
        self.instructions_executed += 1;
        self.call_instructions += 1;
        self.total_cycles += 2;
        if current_depth > self.peak_ring_depth {
            self.peak_ring_depth = current_depth;
        }
    }

    pub fn record_ret(&mut self) {
        self.instructions_executed += 1;
        self.ret_instructions += 1;
        self.total_cycles += 2;
    }

    pub fn record_misc(&mut self, cost_cycles: u64) {
        self.instructions_executed += 1;
        self.total_cycles += cost_cycles;
    }
}

impl fmt::Display for Metrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "================= Ω-Core Hardware Metrics =================")?;
        writeln!(f, "Total Elapsed Cycles:               {}", self.total_cycles)?;
        writeln!(f, "Instructions Executed:              {}", self.instructions_executed)?;
        writeln!(f, "  - ALU Operations:                 {}", self.alu_instructions)?;
        writeln!(f, "  - Memory Accesses:                {}", self.mem_instructions)?;
        writeln!(f, "  - Branch Instructions:            {}", self.branch_instructions)?;
        writeln!(f, "    * Branches Taken:               {} ({:.1}%)",
            self.branches_taken,
            if self.branch_instructions > 0 {
                (self.branches_taken as f64 / self.branch_instructions as f64) * 100.0
            } else { 0.0 }
        )?;
        writeln!(f, "  - Call / Ret Instructions:        {} / {}", self.call_instructions, self.ret_instructions)?;
        writeln!(f, "Stall Penalties:")?;
        writeln!(f, "  - Bank Lockout Stalls:            {} cycles", self.bank_stall_cycles)?;
        writeln!(f, "  - Branch Mispredict Penalty:      {} cycles", self.branch_mispredict_stall_cycles)?;
        writeln!(f, "Peak Hardware Watermarks:")?;
        writeln!(f, "  - Memory High-Water Mark:         0x{:04X} (word {})", self.memory_high_water_mark, self.memory_high_water_mark)?;
        writeln!(f, "  - Peak Ring Buffer Depth:         {} / 8", self.peak_ring_depth)?;
        writeln!(f, "Physical Bank Distribution:")?;
        for (b, count) in self.bank_access_counts.iter().enumerate() {
            writeln!(f, "  Bank {}: {:>8} accesses", b, count)?;
        }
        writeln!(f, "Dynamic Host Allocations:           0 (Bare Metal)")?;
        writeln!(f, "===========================================================")?;
        Ok(())
    }
}
