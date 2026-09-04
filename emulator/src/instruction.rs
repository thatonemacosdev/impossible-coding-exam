//! Instruction representation and AST for Ω-Core.

use crate::types::{BranchCond, RegDest, RegSrc, SourceOperand, Word24};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Alu3Op {
    Add,
    Sub,
    Mul,
    Mulh,
    Divs,
    Mods,
    Divu,
    Modu,
    Band,
    Bor,
    Bxor,
    Shl,
    Sar,
    Slr,
    Rol,
    Ror,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Alu2Op {
    Bnot,
    Rev,
    Clz,
    Popcnt,
    Sext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Label(String),
    Offset(i32),
    Absolute(usize),
    Reg(RegSrc),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    Nop,
    Alu3 {
        op: Alu3Op,
        dest: RegDest,
        src1: RegSrc,
        src2: SourceOperand,
    },
    Alu2 {
        op: Alu2Op,
        dest: RegDest,
        src: RegSrc,
    },
    Mov {
        dest: RegDest,
        src: SourceOperand,
    },
    Ldw {
        dest: RegDest,
        base: RegSrc,
        offset: i32,
    },
    Stw {
        base: RegSrc,
        offset: i32,
        val: RegSrc,
    },
    Xchg {
        dest: RegDest,
        base: RegSrc,
        offset: i32,
    },
    Branch {
        cond: BranchCond,
        src1: RegSrc,
        src2: SourceOperand,
        target: Target,
    },
    Jmp {
        target: Target,
    },
    Call {
        target: Target,
    },
    Ret,
    RbSave {
        base: RegSrc,
    },
    RbRst {
        base: RegSrc,
    },
    Trap {
        code: Word24,
    },
}
