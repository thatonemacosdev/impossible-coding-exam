//! Deterministic execution engine evaluating formal operational state transitions.

use crate::instruction::{Alu2Op, Alu3Op, Instruction, Target};
use crate::state::{State, MEMORY_SIZE};
use crate::types::{BranchCond, SourceOperand, TrapReason, Word24};
use std::collections::HashMap;

pub struct Executor;

impl Executor {
    /// Executes a single instruction step against the machine state σ.
    pub fn step(
        state: &mut State,
        instructions: &[Instruction],
        symbols: &HashMap<String, u32>,
    ) -> Result<(), TrapReason> {
        if let Some(ref reason) = state.halted {
            return Err(reason.clone());
        }

        if state.pc >= instructions.len() {
            // Execution reached beyond code segment -> clean halt with return code in r0
            let ret_code = state.r[0].to_u32();
            let reason = TrapReason::Halt(ret_code);
            state.halted = Some(reason.clone());
            return Err(reason);
        }

        let inst = &instructions[state.pc];
        match inst {
            Instruction::Nop => {
                state.metrics.record_misc(1);
                state.pc += 1;
            }

            Instruction::Trap { code } => {
                state.metrics.record_misc(1);
                let reason = TrapReason::Halt(code.to_u32());
                state.halted = Some(reason.clone());
                return Err(reason);
            }

            Instruction::Mov { dest, src } => {
                let val = match src {
                    SourceOperand::Reg(reg_src) => state.eval_reg_src(reg_src),
                    SourceOperand::Imm(imm) => *imm,
                };
                state.write_reg_dest(dest, val);
                state.update_flags_result(val);
                state.metrics.record_alu(1);
                state.pc += 1;
            }

            Instruction::Alu2 { op, dest, src } => {
                let val = state.eval_reg_src(src);
                let res = match op {
                    Alu2Op::Bnot => val.bnot(),
                    Alu2Op::Rev => val.rev(),
                    Alu2Op::Clz => val.clz(),
                    Alu2Op::Popcnt => val.popcnt(),
                    Alu2Op::Sext => Word24::from_i32(val.to_i32()),
                };
                state.write_reg_dest(dest, res);
                state.update_flags_result(res);
                state.metrics.record_alu(1);
                state.pc += 1;
            }

            Instruction::Alu3 {
                op,
                dest,
                src1,
                src2,
            } => {
                // Strict evaluation order: src1 then src2
                let v1 = state.eval_reg_src(src1);
                let v2 = match src2 {
                    SourceOperand::Reg(reg_src) => state.eval_reg_src(reg_src),
                    SourceOperand::Imm(imm) => *imm,
                };

                let cost: u64;
                let res = match op {
                    Alu3Op::Add => {
                        let (sum, carry) = v1.add(v2);
                        state.flags.c = carry;
                        cost = 1;
                        sum
                    }
                    Alu3Op::Sub => {
                        let (diff, borrow) = v1.sub(v2);
                        state.flags.c = borrow;
                        cost = 1;
                        diff
                    }
                    Alu3Op::Mul => {
                        cost = 2;
                        v1.mul(v2)
                    }
                    Alu3Op::Mulh => {
                        cost = 2;
                        v1.mulh(v2)
                    }
                    Alu3Op::Divs => {
                        cost = 8;
                        match v1.divs(v2) {
                            Ok(res) => res,
                            Err(trap) => {
                                state.metrics.record_alu(1);
                                state.halted = Some(trap.clone());
                                return Err(trap);
                            }
                        }
                    }
                    Alu3Op::Mods => {
                        cost = 8;
                        match v1.mods(v2) {
                            Ok(res) => res,
                            Err(trap) => {
                                state.metrics.record_alu(1);
                                state.halted = Some(trap.clone());
                                return Err(trap);
                            }
                        }
                    }
                    Alu3Op::Divu => {
                        cost = 8;
                        match v1.divu(v2) {
                            Ok(res) => res,
                            Err(trap) => {
                                state.metrics.record_alu(1);
                                state.halted = Some(trap.clone());
                                return Err(trap);
                            }
                        }
                    }
                    Alu3Op::Modu => {
                        cost = 8;
                        match v1.modu(v2) {
                            Ok(res) => res,
                            Err(trap) => {
                                state.metrics.record_alu(1);
                                state.halted = Some(trap.clone());
                                return Err(trap);
                            }
                        }
                    }
                    Alu3Op::Band => {
                        cost = 1;
                        v1.band(v2)
                    }
                    Alu3Op::Bor => {
                        cost = 1;
                        v1.bor(v2)
                    }
                    Alu3Op::Bxor => {
                        cost = 1;
                        v1.bxor(v2)
                    }
                    Alu3Op::Shl => {
                        cost = 1;
                        v1.shl(v2)
                    }
                    Alu3Op::Sar => {
                        cost = 1;
                        v1.sar(v2)
                    }
                    Alu3Op::Slr => {
                        cost = 1;
                        v1.slr(v2)
                    }
                    Alu3Op::Rol => {
                        cost = 1;
                        v1.rol(v2)
                    }
                    Alu3Op::Ror => {
                        cost = 1;
                        v1.ror(v2)
                    }
                };

                state.write_reg_dest(dest, res);
                state.update_flags_result(res);
                state.metrics.record_alu(cost);
                state.pc += 1;
            }

            Instruction::Ldw { dest, base, offset } => {
                let base_val = state.eval_reg_src(base);
                let addr_i64 = (base_val.to_u32() as i64) + (*offset as i64);
                if addr_i64 < 0 || addr_i64 >= (MEMORY_SIZE as i64) {
                    let trap = TrapReason::AddrOutOfBounds(addr_i64 as u32);
                    state.halted = Some(trap.clone());
                    return Err(trap);
                }
                let target_addr = addr_i64 as u32;

                let (stall, total_latency) =
                    state.banks.access(target_addr, state.metrics.total_cycles);
                let val = state.mem[target_addr as usize];
                state.write_reg_dest(dest, val);
                state.update_flags_result(val);
                state.metrics.record_mem(target_addr, stall, total_latency);
                state.pc += 1;
            }

            Instruction::Stw { base, offset, val } => {
                let base_val = state.eval_reg_src(base);
                let store_val = state.eval_reg_src(val);
                let addr_i64 = (base_val.to_u32() as i64) + (*offset as i64);
                if addr_i64 < 0 || addr_i64 >= (MEMORY_SIZE as i64) {
                    let trap = TrapReason::AddrOutOfBounds(addr_i64 as u32);
                    state.halted = Some(trap.clone());
                    return Err(trap);
                }
                let target_addr = addr_i64 as u32;

                let (stall, total_latency) =
                    state.banks.access(target_addr, state.metrics.total_cycles);
                state.mem[target_addr as usize] = store_val;
                state.metrics.record_mem(target_addr, stall, total_latency);
                state.pc += 1;
            }

            Instruction::Xchg { dest, base, offset } => {
                let base_val = state.eval_reg_src(base);
                let addr_i64 = (base_val.to_u32() as i64) + (*offset as i64);
                if addr_i64 < 0 || addr_i64 >= (MEMORY_SIZE as i64) {
                    let trap = TrapReason::AddrOutOfBounds(addr_i64 as u32);
                    state.halted = Some(trap.clone());
                    return Err(trap);
                }
                let target_addr = addr_i64 as u32;

                let (stall, total_latency) =
                    state.banks.access(target_addr, state.metrics.total_cycles);
                let mem_val = state.mem[target_addr as usize];
                let reg_val = state.r[dest.reg.index()];
                state.mem[target_addr as usize] = reg_val;
                state.write_reg_dest(dest, mem_val);
                state.metrics.record_mem(target_addr, stall, total_latency);
                state.pc += 1;
            }

            Instruction::RbSave { base } => {
                let base_val = state.eval_reg_src(base);
                let base_addr = base_val.to_u32() as usize;
                if base_addr + 8 > MEMORY_SIZE {
                    let trap = TrapReason::AddrOutOfBounds((base_addr + 8) as u32);
                    state.halted = Some(trap.clone());
                    return Err(trap);
                }
                for i in 0..8 {
                    state.mem[base_addr + i] = state.rb[i];
                }
                state.metrics.record_misc(8);
                state.pc += 1;
            }

            Instruction::RbRst { base } => {
                let base_val = state.eval_reg_src(base);
                let base_addr = base_val.to_u32() as usize;
                if base_addr + 8 > MEMORY_SIZE {
                    let trap = TrapReason::AddrOutOfBounds((base_addr + 8) as u32);
                    state.halted = Some(trap.clone());
                    return Err(trap);
                }
                for i in 0..8 {
                    state.rb[i] = state.mem[base_addr + i];
                }
                state.metrics.record_misc(8);
                state.pc += 1;
            }

            Instruction::Branch {
                cond,
                src1,
                src2,
                target,
            } => {
                let v1 = state.eval_reg_src(src1);
                let v2 = match src2 {
                    SourceOperand::Reg(reg_src) => state.eval_reg_src(reg_src),
                    SourceOperand::Imm(imm) => *imm,
                };

                let taken = match cond {
                    BranchCond::Eq => v1.0 == v2.0,
                    BranchCond::Ne => v1.0 != v2.0,
                    BranchCond::Lt => v1.to_i32() < v2.to_i32(),
                    BranchCond::Le => v1.to_i32() <= v2.to_i32(),
                    BranchCond::Gt => v1.to_i32() > v2.to_i32(),
                    BranchCond::Ge => v1.to_i32() >= v2.to_i32(),
                    BranchCond::Ltu => v1.0 < v2.0,
                    BranchCond::Leu => v1.0 <= v2.0,
                    BranchCond::Gtu => v1.0 > v2.0,
                    BranchCond::Geu => v1.0 >= v2.0,
                };

                if taken {
                    let target_pc = Self::resolve_target(target, state, symbols)?;
                    state.pc = target_pc;
                    state.metrics.record_branch(true);
                } else {
                    state.pc += 1;
                    state.metrics.record_branch(false);
                }
            }

            Instruction::Jmp { target } => {
                let target_pc = Self::resolve_target(target, state, symbols)?;
                state.pc = target_pc;
                state.metrics.record_misc(1);
            }

            Instruction::Call { target } => {
                let target_pc = Self::resolve_target(target, state, symbols)?;
                state.push_ring(Word24::from_u32((state.pc + 1) as u32));
                state.metrics.record_call(state.call_depth);
                state.pc = target_pc;
            }

            Instruction::Ret => {
                let ret_addr = state.pop_ring();
                state.pc = ret_addr.to_u32() as usize;
                state.metrics.record_ret();
            }
        }

        Ok(())
    }

    fn resolve_target(
        target: &Target,
        state: &mut State,
        symbols: &HashMap<String, u32>,
    ) -> Result<usize, TrapReason> {
        match target {
            Target::Absolute(addr) => Ok(*addr),
            Target::Offset(off) => Ok(((state.pc as i32) + off) as usize),
            Target::Reg(reg_src) => {
                let val = state.eval_reg_src(reg_src);
                Ok(val.to_u32() as usize)
            }
            Target::Label(name) => symbols.get(name).map(|&a| a as usize).ok_or_else(|| {
                TrapReason::IllegalInstruction(format!("Unresolved branch label '{}'", name))
            }),
        }
    }

    /// Executes instructions until termination or until max_cycles is exceeded.
    pub fn run(
        state: &mut State,
        instructions: &[Instruction],
        symbols: &HashMap<String, u32>,
        max_cycles: u64,
    ) -> Result<u32, TrapReason> {
        while state.metrics.total_cycles < max_cycles {
            match Self::step(state, instructions, symbols) {
                Ok(()) => {}
                Err(TrapReason::Halt(code)) => return Ok(code),
                Err(err) => return Err(err),
            }
        }
        let trap = TrapReason::MaxCyclesExceeded(max_cycles);
        state.halted = Some(trap.clone());
        Err(trap)
    }
}
