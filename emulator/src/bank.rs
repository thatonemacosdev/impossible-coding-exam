//! Asymmetric 7-Bank Interleaved Memory Controller with Dynamic Lockout Stalls.

/// The prime modulus number of physical memory banks.
pub const NUM_BANKS: usize = 7;
/// Number of cycles a bank remains locked after a memory access completes.
pub const BANK_LOCKOUT_CYCLES: u64 = 4;
/// Base access latency for uncontended memory read/write.
pub const MEM_BASE_LATENCY: u64 = 2;

#[derive(Clone, Debug)]
pub struct BankTracker {
    /// Timestamp (in cycles) at which each bank becomes free to accept a new request.
    lockout_until: [u64; NUM_BANKS],
    /// Access counters per bank for diagnostic and metric tracking.
    access_counts: [u64; NUM_BANKS],
}

impl Default for BankTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BankTracker {
    pub fn new() -> Self {
        Self {
            lockout_until: [0; NUM_BANKS],
            access_counts: [0; NUM_BANKS],
        }
    }

    /// Computes the physical bank index for a 16-bit address ($A \pmod 7$).
    #[inline(always)]
    pub fn bank_for_address(addr: u32) -> usize {
        (addr as usize) % NUM_BANKS
    }

    /// Evaluates memory access at the current machine cycle.
    ///
    /// Returns a tuple `(stall_cycles, total_latency)`:
    /// - `stall_cycles`: Pipeline stall penalty waiting for bank lockout to clear.
    /// - `total_latency`: Total elapsed cycles from instruction dispatch to memory completion
    ///   (`stall_cycles + MEM_BASE_LATENCY`).
    ///
    /// Updates the bank's lockout timestamp to `completion_time + BANK_LOCKOUT_CYCLES`.
    pub fn access(&mut self, addr: u32, current_cycle: u64) -> (u64, u64) {
        let bank = Self::bank_for_address(addr);
        self.access_counts[bank] += 1;

        let free_at = self.lockout_until[bank];
        let stall = free_at.saturating_sub(current_cycle);

        let completion_cycle = current_cycle + stall + MEM_BASE_LATENCY;
        self.lockout_until[bank] = completion_cycle + BANK_LOCKOUT_CYCLES;

        (stall, stall + MEM_BASE_LATENCY)
    }

    pub fn access_counts(&self) -> &[u64; NUM_BANKS] {
        &self.access_counts
    }

    pub fn lockout_table(&self) -> &[u64; NUM_BANKS] {
        &self.lockout_until
    }

    pub fn reset(&mut self) {
        self.lockout_until = [0; NUM_BANKS];
        self.access_counts = [0; NUM_BANKS];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bank_modulo() {
        assert_eq!(BankTracker::bank_for_address(0), 0);
        assert_eq!(BankTracker::bank_for_address(1), 1);
        assert_eq!(BankTracker::bank_for_address(6), 6);
        assert_eq!(BankTracker::bank_for_address(7), 0);
        assert_eq!(BankTracker::bank_for_address(14), 0);
        assert_eq!(BankTracker::bank_for_address(15), 1);
    }

    #[test]
    fn test_uncontended_access() {
        let mut bt = BankTracker::new();
        let (stall, total) = bt.access(0, 0);
        assert_eq!(stall, 0);
        assert_eq!(total, 2);
        // Bank 0 complete at t=2, locked until t=6.
        assert_eq!(bt.lockout_table()[0], 6);
    }

    #[test]
    fn test_bank_collision_stall() {
        let mut bt = BankTracker::new();
        // First access at t=0 to bank 0
        let (stall1, total1) = bt.access(0, 0);
        assert_eq!(stall1, 0);
        assert_eq!(total1, 2); // completes at t=2, locked until t=6

        // Second access at t=2 to bank 0 (address 7)
        let (stall2, total2) = bt.access(7, 2);
        assert_eq!(stall2, 4); // locked until 6, so stall = 6 - 2 = 4
        assert_eq!(total2, 6); // stall 4 + latency 2 = 6 cycles -> completes at t=8
        assert_eq!(bt.lockout_table()[0], 12); // locked until 8 + 4 = 12
    }

    #[test]
    fn test_independent_bank_no_stall() {
        let mut bt = BankTracker::new();
        // Access bank 0 at t=0
        bt.access(0, 0);
        // Access bank 1 at t=1 (address 1)
        let (stall, total) = bt.access(1, 1);
        assert_eq!(stall, 0);
        assert_eq!(total, 2);
    }
}
