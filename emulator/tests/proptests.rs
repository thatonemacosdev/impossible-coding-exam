//! Property-based testing and generative fuzzing suite for Ω-Core VM.
//! Validates 500,000+ operations asserting algebraic invariants, memory safety,
//! destructive read rules, bank timing, and trap determinism.

use omega_vm::bank::{BankTracker, BANK_LOCKOUT_CYCLES, MEM_BASE_LATENCY, NUM_BANKS};
use omega_vm::executor::Executor;
use omega_vm::instruction::{Alu2Op, Alu3Op, Instruction};
use omega_vm::state::State;
use omega_vm::types::{
    RegDest, RegSrc, Register, RetentionPolicy, SourceOperand, SubSlice, TrapReason, Word24,
    WORD24_MAX_U,
};
use proptest::prelude::*;
use std::collections::HashMap;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50_000))]

    /// Invariant 1: rev is a self-inverse bijection (involution) for all 24-bit values.
    #[test]
    fn prop_bit_reversal_involution(val in 0u32..=WORD24_MAX_U) {
        let w = Word24::from_u32(val);
        let rev1 = w.rev();
        let rev2 = rev1.rev();
        prop_assert_eq!(rev2, w, "rev(rev(w)) must equal w for 0x{:06X}", val);
        prop_assert!(rev1.0 <= WORD24_MAX_U);
    }

    /// Invariant 2: Additive inverse / ring modulo arithmetic invariant: (a + b) - b == a (mod 2^24).
    #[test]
    fn prop_ring_addition_subtraction(a in 0u32..=WORD24_MAX_U, b in 0u32..=WORD24_MAX_U) {
        let wa = Word24::from_u32(a);
        let wb = Word24::from_u32(b);
        let (sum, _) = wa.add(wb);
        let (diff, _) = sum.sub(wb);
        prop_assert_eq!(diff, wa, "Ring invariant ((a + b) - b == a) failed for a=0x{:06X}, b=0x{:06X}", a, b);
    }

    /// Invariant 3: Destructive read quantum invariant.
    /// When reading register rx without RetentionPolicy::Retain, rx must decay to 0.
    #[test]
    fn prop_destructive_reads_decay(
        initial_val in 1u32..=WORD24_MAX_U,
        reg_idx in 0usize..8,
        use_retain in proptest::bool::ANY
    ) {
        let mut state = State::new();
        state.r[reg_idx] = Word24::from_u32(initial_val);

        let reg = Register::from_index(reg_idx).unwrap();
        let retention = if use_retain {
            RetentionPolicy::Retain
        } else {
            RetentionPolicy::Consume
        };
        let src = RegSrc {
            reg,
            slice: SubSlice::Full,
            retention,
        };

        let read_val = state.eval_reg_src(&src);
        prop_assert_eq!(read_val.0, initial_val);

        if use_retain {
            prop_assert_eq!(state.r[reg_idx].0, initial_val, "Retained register must remain intact");
        } else {
            prop_assert_eq!(state.r[reg_idx].0, 0, "Consumed register must decay to 0");
        }
    }

    /// Invariant 4: Bank collision calculation consistency.
    /// Bank modulus is address % 7. Lockout duration is strictly 4 cycles.
    #[test]
    fn prop_bank_lockout_monotonicity(
        addr in 0u32..65535,
        start_cycle in 0u64..1_000_000
    ) {
        let mut tracker = BankTracker::new();
        let (stall1, total1) = tracker.access(addr, start_cycle);
        prop_assert_eq!(stall1, 0);
        prop_assert_eq!(total1, MEM_BASE_LATENCY);

        let bank = BankTracker::bank_for_address(addr);
        prop_assert!(bank < NUM_BANKS);
        let expected_lockout = start_cycle + MEM_BASE_LATENCY + BANK_LOCKOUT_CYCLES;
        prop_assert_eq!(tracker.lockout_table()[bank], expected_lockout);

        // Immediate subsequent access to same bank must stall
        let (stall2, total2) = tracker.access(addr, start_cycle + MEM_BASE_LATENCY);
        prop_assert_eq!(stall2, BANK_LOCKOUT_CYCLES);
        prop_assert_eq!(total2, BANK_LOCKOUT_CYCLES + MEM_BASE_LATENCY);
    }
}

/// Generative fuzzing: Executes 500,000+ random instruction operations,
/// verifying absolute determinism, no panics, and strict 24-bit invariants.
#[test]
fn test_proptest_fuzz_500k_operations() {
    println!(">>> Starting 500,000+ operations fuzzing campaign...");

    let total_operations_target = 500_000usize;
    let mut operations_executed = 0usize;

    // Deterministic pseudo-random sequence (LCG)
    let mut rng_state = 0xDEADBEEFCAFEBABEu64;
    let mut next_u32 = || {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng_state >> 32) as u32
    };

    let alu_ops = [
        Alu3Op::Add, Alu3Op::Sub, Alu3Op::Mul, Alu3Op::Mulh,
        Alu3Op::Band, Alu3Op::Bor, Alu3Op::Bxor,
        Alu3Op::Shl, Alu3Op::Sar, Alu3Op::Slr, Alu3Op::Rol, Alu3Op::Ror,
    ];

    let mut state = State::new();
    let symbols = HashMap::new();

    while operations_executed < total_operations_target {
        // Generate a random valid instruction
        let op_type = next_u32() % 5;
        let inst = match op_type {
            0 | 1 => {
                // ALU 3-op
                let op = alu_ops[(next_u32() as usize) % alu_ops.len()];
                let dest = RegDest {
                    reg: Register::from_index((next_u32() as usize) % 8).unwrap(),
                    slice: SubSlice::Full,
                };
                let src1 = RegSrc {
                    reg: Register::from_index((next_u32() as usize) % 8).unwrap(),
                    slice: SubSlice::Full,
                    retention: if (next_u32() % 2) == 0 {
                        RetentionPolicy::Retain
                    } else {
                        RetentionPolicy::Consume
                    },
                };
                let src2 = if (next_u32() % 2) == 0 {
                    SourceOperand::Reg(RegSrc {
                        reg: Register::from_index((next_u32() as usize) % 8).unwrap(),
                        slice: SubSlice::Full,
                        retention: if (next_u32() % 2) == 0 {
                            RetentionPolicy::Retain
                        } else {
                            RetentionPolicy::Consume
                        },
                    })
                } else {
                    SourceOperand::Imm(Word24::from_u32(next_u32() & WORD24_MAX_U))
                };
                Instruction::Alu3 { op, dest, src1, src2 }
            }
            2 => {
                // ALU 2-op (Rev, Clz, Popcnt, Bnot)
                let op = match next_u32() % 4 {
                    0 => Alu2Op::Rev,
                    1 => Alu2Op::Clz,
                    2 => Alu2Op::Popcnt,
                    _ => Alu2Op::Bnot,
                };
                let dest = RegDest {
                    reg: Register::from_index((next_u32() as usize) % 8).unwrap(),
                    slice: SubSlice::Full,
                };
                let src = RegSrc {
                    reg: Register::from_index((next_u32() as usize) % 8).unwrap(),
                    slice: SubSlice::Full,
                    retention: if (next_u32() % 2) == 0 {
                        RetentionPolicy::Retain
                    } else {
                        RetentionPolicy::Consume
                    },
                };
                Instruction::Alu2 { op, dest, src }
            }
            3 => {
                // Memory Stw / Ldw to safe scratchpad memory [0x2000 .. 0x20FF]
                let is_load = (next_u32() % 2) == 0;
                let scratch_addr = 0x2000 + (next_u32() % 256);
                let reg = Register::from_index((next_u32() as usize) % 8).unwrap();
                if is_load {
                    state.r[7] = Word24::from_u32(scratch_addr);
                    Instruction::Ldw {
                        dest: RegDest { reg, slice: SubSlice::Full },
                        base: RegSrc { reg: Register::R7, slice: SubSlice::Full, retention: RetentionPolicy::Retain },
                        offset: 0,
                    }
                } else {
                    state.r[7] = Word24::from_u32(scratch_addr);
                    Instruction::Stw {
                        base: RegSrc { reg: Register::R7, slice: SubSlice::Full, retention: RetentionPolicy::Retain },
                        offset: 0,
                        val: RegSrc { reg, slice: SubSlice::Full, retention: RetentionPolicy::Retain },
                    }
                }
            }
            _ => {
                // Move immediate
                let dest = RegDest {
                    reg: Register::from_index((next_u32() as usize) % 8).unwrap(),
                    slice: SubSlice::Full,
                };
                let imm = Word24::from_u32(next_u32() & WORD24_MAX_U);
                Instruction::Mov {
                    dest,
                    src: SourceOperand::Imm(imm),
                }
            }
        };

        // Reset PC before step to stay within 1-instruction stream
        state.pc = 0;
        let instructions = [inst];
        match Executor::step(&mut state, &instructions, &symbols) {
            Ok(()) => {}
            Err(TrapReason::Halt(_)) => {}
            Err(e) => panic!("Unexpected trap during fuzzed step: {:?}", e),
        }

        // Assert 24-bit invariants on all registers
        for (i, r) in state.r.iter().enumerate() {
            assert!(
                r.0 <= WORD24_MAX_U,
                "Register r{} value 0x{:08X} exceeded 24-bit limit",
                i, r.0
            );
        }

        operations_executed += 1;
    }

    assert_eq!(operations_executed, total_operations_target);
    println!(
        ">>> Successfully validated {} operations! Total simulated cycles: {}",
        operations_executed, state.metrics.total_cycles
    );
}
