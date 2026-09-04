//! Two-pass assembler and parser for the Ω-Core assembly grammar.

use crate::instruction::{Alu2Op, Alu3Op, Instruction, Target};
use crate::types::{
    BranchCond, RegDest, RegSrc, Register, RetentionPolicy, SourceOperand, SubSlice, Word24,
};
use std::collections::HashMap;

#[derive(Debug)]
pub struct AssembledProgram {
    pub instructions: Vec<Instruction>,
    /// Initialized memory words: (address, value)
    pub data_segment: Vec<(u32, Word24)>,
    pub symbols: HashMap<String, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line_number: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse Error at line {}: {}", self.line_number, self.message)
    }
}

impl std::error::Error for ParseError {}

pub struct Parser;

impl Parser {
    pub fn assemble(source: &str) -> Result<AssembledProgram, ParseError> {
        let mut symbols: HashMap<String, u32> = HashMap::new();
        let mut raw_lines = Vec::new();

        enum Section {
            Text,
            Data,
        }
        let mut current_section = Section::Text;
        let mut code_pc: u32 = 0;
        let mut data_addr: u32 = 0x1000;
        let mut data_segment: Vec<(u32, Word24)> = Vec::new();

        // Pass 1: Strip comments, collect labels, directives, and raw instruction lines
        for (idx, original_line) in source.lines().enumerate() {
            let line_num = idx + 1;
            let mut line = original_line.trim();

            // Strip comments
            if let Some(pos) = line.find(';') {
                line = line[..pos].trim();
            }
            if let Some(pos) = line.find("//") {
                line = line[..pos].trim();
            }

            if line.is_empty() {
                continue;
            }

            // Check for labels (e.g. "loop:" or "loop: add r0, r1, r2")
            while let Some(colon_pos) = line.find(':') {
                // Verify colon is not part of something else
                let label_name = line[..colon_pos].trim();
                if Self::is_valid_ident(label_name) {
                    let target_addr = match current_section {
                        Section::Text => code_pc,
                        Section::Data => data_addr,
                    };
                    if symbols.insert(label_name.to_string(), target_addr).is_some() {
                        return Err(ParseError {
                            line_number: line_num,
                            message: format!("Duplicate label '{}'", label_name),
                        });
                    }
                    line = line[colon_pos + 1..].trim();
                } else {
                    break;
                }
            }

            if line.is_empty() {
                continue;
            }

            // Directives
            if line.starts_with('.') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                match parts[0] {
                    ".text" => current_section = Section::Text,
                    ".data" => current_section = Section::Data,
                    ".org" => {
                        if parts.len() < 2 {
                            return Err(ParseError {
                                line_number: line_num,
                                message: "Missing address for .org".into(),
                            });
                        }
                        let addr = Self::parse_literal(parts[1], line_num)?;
                        match current_section {
                            Section::Text => code_pc = addr.to_u32(),
                            Section::Data => data_addr = addr.to_u32(),
                        }
                    }
                    ".word" => {
                        let rest = line[5..].trim();
                        let items = rest.split(',');
                        for item in items {
                            let lit = Self::parse_literal(item.trim(), line_num)?;
                            data_segment.push((data_addr, lit));
                            data_addr += 1;
                        }
                    }
                    ".space" => {
                        if parts.len() < 2 {
                            return Err(ParseError {
                                line_number: line_num,
                                message: "Missing size for .space".into(),
                            });
                        }
                        let count = Self::parse_literal(parts[1], line_num)?.to_u32();
                        for _ in 0..count {
                            data_segment.push((data_addr, Word24::ZERO));
                            data_addr += 1;
                        }
                    }
                    ".entry" | ".globl" | ".global" => {
                        // Entry point declaration or export directive
                    }
                    other => {
                        return Err(ParseError {
                            line_number: line_num,
                            message: format!("Unknown directive '{}'", other),
                        })
                    }
                }
                continue;
            }

            // If in Text section, this is an instruction
            raw_lines.push((line_num, code_pc as usize, line.to_string()));
            code_pc += 1;
        }

        // Pass 2: Parse instructions and resolve symbols
        let mut instructions = Vec::with_capacity(raw_lines.len());
        for (line_num, current_pc, line_text) in raw_lines {
            let inst = Self::parse_instruction(&line_text, current_pc, &symbols, line_num)?;
            instructions.push(inst);
        }

        Ok(AssembledProgram {
            instructions,
            data_segment,
            symbols,
        })
    }

    fn is_valid_ident(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let first = s.chars().next().unwrap();
        (first.is_alphabetic() || first == '_')
            && s.chars().all(|c| c.is_alphanumeric() || c == '_')
    }

    fn parse_literal(s: &str, line_num: usize) -> Result<Word24, ParseError> {
        let s = s.trim();
        let s = s.strip_prefix('#').unwrap_or(s);
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16)
                .map(Word24::from_u32)
                .map_err(|e| ParseError {
                    line_number: line_num,
                    message: format!("Invalid hex literal '{}': {}", s, e),
                })
        } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
            u32::from_str_radix(bin, 2)
                .map(Word24::from_u32)
                .map_err(|e| ParseError {
                    line_number: line_num,
                    message: format!("Invalid bin literal '{}': {}", s, e),
                })
        } else {
            // Decimal (can be signed)
            s.parse::<i32>()
                .map(Word24::from_i32)
                .map_err(|e| ParseError {
                    line_number: line_num,
                    message: format!("Invalid decimal literal '{}': {}", s, e),
                })
        }
    }

    fn parse_reg_dest(s: &str, line_num: usize) -> Result<RegDest, ParseError> {
        let s = s.trim();
        if s.starts_with('@') {
            return Err(ParseError {
                line_number: line_num,
                message: format!("Destination register '{}' cannot have retention sigil '@'", s),
            });
        }
        let (reg_str, slice) = Self::extract_subslice(s, line_num)?;
        let reg = Self::parse_raw_reg(reg_str, line_num)?;
        Ok(RegDest { reg, slice })
    }

    fn parse_reg_src(s: &str, line_num: usize) -> Result<RegSrc, ParseError> {
        let mut s = s.trim();
        let retention = if s.starts_with('@') {
            s = &s[1..];
            RetentionPolicy::Retain
        } else {
            RetentionPolicy::Consume
        };
        let (reg_str, slice) = Self::extract_subslice(s, line_num)?;
        let reg = Self::parse_raw_reg(reg_str, line_num)?;
        Ok(RegSrc {
            reg,
            slice,
            retention,
        })
    }

    fn extract_subslice(s: &str, line_num: usize) -> Result<(&str, SubSlice), ParseError> {
        if let Some(dot_pos) = s.find('.') {
            let reg_part = &s[..dot_pos];
            let sub_part = &s[dot_pos + 1..];
            let slice = match sub_part {
                "l" => SubSlice::L,
                "h" => SubSlice::H,
                "b0" => SubSlice::B0,
                "b1" => SubSlice::B1,
                "b2" => SubSlice::B2,
                other => {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("Unknown subslice '.{}'", other),
                    })
                }
            };
            Ok((reg_part, slice))
        } else {
            Ok((s, SubSlice::Full))
        }
    }

    fn parse_raw_reg(s: &str, line_num: usize) -> Result<Register, ParseError> {
        match s {
            "r0" => Ok(Register::R0),
            "r1" => Ok(Register::R1),
            "r2" => Ok(Register::R2),
            "r3" => Ok(Register::R3),
            "r4" => Ok(Register::R4),
            "r5" => Ok(Register::R5),
            "r6" => Ok(Register::R6),
            "r7" => Ok(Register::R7),
            other => Err(ParseError {
                line_number: line_num,
                message: format!("Invalid register name '{}'", other),
            }),
        }
    }

    fn parse_source_operand(
        s: &str,
        symbols: &HashMap<String, u32>,
        line_num: usize,
    ) -> Result<SourceOperand, ParseError> {
        let s = s.trim();
        // Check if it's a register: starts with '@' or 'r[0-7]'
        if s.starts_with('@') || (s.starts_with('r') && s.len() >= 2 && s.chars().nth(1).unwrap().is_ascii_digit()) {
            let reg_src = Self::parse_reg_src(s, line_num)?;
            Ok(SourceOperand::Reg(reg_src))
        } else if let Some(&sym_addr) = symbols.get(s) {
            Ok(SourceOperand::Imm(Word24::from_u32(sym_addr)))
        } else {
            let lit = Self::parse_literal(s, line_num)?;
            Ok(SourceOperand::Imm(lit))
        }
    }

    fn parse_mem_operand(s: &str, line_num: usize) -> Result<(RegSrc, i32), ParseError> {
        let s = s.trim();
        if !s.starts_with('[') || !s.ends_with(']') {
            return Err(ParseError {
                line_number: line_num,
                message: format!("Expected memory operand inside brackets '[...]', found '{}'", s),
            });
        }
        let inner = s[1..s.len() - 1].trim();

        // Check for '+' or '-'
        if let Some(plus_pos) = inner.find('+') {
            let reg_str = inner[..plus_pos].trim();
            let off_str = inner[plus_pos + 1..].trim();
            let reg = Self::parse_reg_src(reg_str, line_num)?;
            let off = Self::parse_literal(off_str, line_num)?.to_i32();
            Ok((reg, off))
        } else if let Some(minus_pos) = inner.find('-') {
            let reg_str = inner[..minus_pos].trim();
            let off_str = inner[minus_pos + 1..].trim();
            let reg = Self::parse_reg_src(reg_str, line_num)?;
            let off = -(Self::parse_literal(off_str, line_num)?.to_i32());
            Ok((reg, off))
        } else {
            let reg = Self::parse_reg_src(inner, line_num)?;
            Ok((reg, 0))
        }
    }

    fn parse_target(
        s: &str,
        symbols: &HashMap<String, u32>,
        line_num: usize,
    ) -> Result<Target, ParseError> {
        let s = s.trim();
        if s.starts_with('@') || (s.starts_with('r') && s.len() >= 2 && s.chars().nth(1).unwrap().is_ascii_digit()) {
            let reg = Self::parse_reg_src(s, line_num)?;
            Ok(Target::Reg(reg))
        } else if let Some(&addr) = symbols.get(s) {
            Ok(Target::Absolute(addr as usize))
        } else if let Ok(lit) = Self::parse_literal(s, line_num) {
            Ok(Target::Absolute(lit.to_u32() as usize))
        } else {
            Ok(Target::Label(s.to_string()))
        }
    }

    fn split_operands(s: &str) -> Vec<&str> {
        let mut operands = Vec::new();
        let mut start = 0;
        let mut in_bracket = false;

        for (i, c) in s.char_indices() {
            match c {
                '[' => in_bracket = true,
                ']' => in_bracket = false,
                ',' if !in_bracket => {
                    operands.push(s[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < s.len() {
            operands.push(s[start..].trim());
        }
        operands
    }

    fn parse_instruction(
        line: &str,
        _current_pc: usize,
        symbols: &HashMap<String, u32>,
        line_num: usize,
    ) -> Result<Instruction, ParseError> {
        let line = line.trim();
        let (opcode, rest) = match line.find(|c: char| c.is_whitespace()) {
            Some(pos) => (&line[..pos], line[pos..].trim()),
            None => (line, ""),
        };

        let operands = if rest.is_empty() {
            Vec::new()
        } else {
            Self::split_operands(rest)
        };

        match opcode {
            "nop" => Ok(Instruction::Nop),
            "ret" => Ok(Instruction::Ret),
            "mov" => {
                if operands.len() != 2 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'mov' expects 2 operands, got {}", operands.len()),
                    });
                }
                let dest = Self::parse_reg_dest(operands[0], line_num)?;
                let src = Self::parse_source_operand(operands[1], symbols, line_num)?;
                Ok(Instruction::Mov { dest, src })
            }
            "ldw" => {
                if operands.len() != 2 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'ldw' expects 2 operands (dest, [base+offset]), got {}", operands.len()),
                    });
                }
                let dest = Self::parse_reg_dest(operands[0], line_num)?;
                let (base, offset) = Self::parse_mem_operand(operands[1], line_num)?;
                Ok(Instruction::Ldw { dest, base, offset })
            }
            "stw" => {
                if operands.len() != 2 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'stw' expects 2 operands ([base+offset], src), got {}", operands.len()),
                    });
                }
                let (base, offset) = Self::parse_mem_operand(operands[0], line_num)?;
                let val = Self::parse_reg_src(operands[1], line_num)?;
                Ok(Instruction::Stw { base, offset, val })
            }
            "xchg" => {
                if operands.len() != 2 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'xchg' expects 2 operands (dest, [base+offset]), got {}", operands.len()),
                    });
                }
                let dest = Self::parse_reg_dest(operands[0], line_num)?;
                let (base, offset) = Self::parse_mem_operand(operands[1], line_num)?;
                Ok(Instruction::Xchg { dest, base, offset })
            }
            "rbsave" => {
                if operands.len() != 1 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'rbsave' expects 1 operand ([base]), got {}", operands.len()),
                    });
                }
                let (base, _) = Self::parse_mem_operand(operands[0], line_num)?;
                Ok(Instruction::RbSave { base })
            }
            "rbrst" => {
                if operands.len() != 1 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'rbrst' expects 1 operand ([base]), got {}", operands.len()),
                    });
                }
                let (base, _) = Self::parse_mem_operand(operands[0], line_num)?;
                Ok(Instruction::RbRst { base })
            }
            "jmp" => {
                if operands.len() != 1 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'jmp' expects 1 operand, got {}", operands.len()),
                    });
                }
                let target = Self::parse_target(operands[0], symbols, line_num)?;
                Ok(Instruction::Jmp { target })
            }
            "call" => {
                if operands.len() != 1 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'call' expects 1 operand, got {}", operands.len()),
                    });
                }
                let target = Self::parse_target(operands[0], symbols, line_num)?;
                Ok(Instruction::Call { target })
            }
            "trap" => {
                if operands.len() != 1 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'trap' expects 1 operand, got {}", operands.len()),
                    });
                }
                let code = Self::parse_literal(operands[0], line_num)?;
                Ok(Instruction::Trap { code })
            }
            // ALU 2-operand
            "bnot" | "rev" | "clz" | "popcnt" | "sext" => {
                if operands.len() != 2 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'{}' expects 2 operands, got {}", opcode, operands.len()),
                    });
                }
                let op = match opcode {
                    "bnot" => Alu2Op::Bnot,
                    "rev" => Alu2Op::Rev,
                    "clz" => Alu2Op::Clz,
                    "popcnt" => Alu2Op::Popcnt,
                    "sext" => Alu2Op::Sext,
                    _ => unreachable!(),
                };
                let dest = Self::parse_reg_dest(operands[0], line_num)?;
                let src = Self::parse_reg_src(operands[1], line_num)?;
                Ok(Instruction::Alu2 { op, dest, src })
            }
            // ALU 3-operand
            "add" | "sub" | "mul" | "mulh" | "divs" | "mods" | "divu" | "modu" | "band"
            | "bor" | "bxor" | "shl" | "sar" | "slr" | "rol" | "ror" => {
                if operands.len() != 3 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'{}' expects 3 operands, got {}", opcode, operands.len()),
                    });
                }
                let op = match opcode {
                    "add" => Alu3Op::Add,
                    "sub" => Alu3Op::Sub,
                    "mul" => Alu3Op::Mul,
                    "mulh" => Alu3Op::Mulh,
                    "divs" => Alu3Op::Divs,
                    "mods" => Alu3Op::Mods,
                    "divu" => Alu3Op::Divu,
                    "modu" => Alu3Op::Modu,
                    "band" => Alu3Op::Band,
                    "bor" => Alu3Op::Bor,
                    "bxor" => Alu3Op::Bxor,
                    "shl" => Alu3Op::Shl,
                    "sar" => Alu3Op::Sar,
                    "slr" => Alu3Op::Slr,
                    "rol" => Alu3Op::Rol,
                    "ror" => Alu3Op::Ror,
                    _ => unreachable!(),
                };
                let dest = Self::parse_reg_dest(operands[0], line_num)?;
                let src1 = Self::parse_reg_src(operands[1], line_num)?;
                let src2 = Self::parse_source_operand(operands[2], symbols, line_num)?;
                Ok(Instruction::Alu3 {
                    op,
                    dest,
                    src1,
                    src2,
                })
            }
            // Branches
            "br.eq" | "br.ne" | "br.lt" | "br.le" | "br.gt" | "br.ge" | "br.ltu" | "br.leu"
            | "br.gtu" | "br.geu" => {
                if operands.len() != 3 {
                    return Err(ParseError {
                        line_number: line_num,
                        message: format!("'{}' expects 3 operands (src1, src2, target), got {}", opcode, operands.len()),
                    });
                }
                let cond = match opcode {
                    "br.eq" => BranchCond::Eq,
                    "br.ne" => BranchCond::Ne,
                    "br.lt" => BranchCond::Lt,
                    "br.le" => BranchCond::Le,
                    "br.gt" => BranchCond::Gt,
                    "br.ge" => BranchCond::Ge,
                    "br.ltu" => BranchCond::Ltu,
                    "br.leu" => BranchCond::Leu,
                    "br.gtu" => BranchCond::Gtu,
                    "br.geu" => BranchCond::Geu,
                    _ => unreachable!(),
                };
                let src1 = Self::parse_reg_src(operands[0], line_num)?;
                let src2 = Self::parse_source_operand(operands[1], symbols, line_num)?;
                let target = Self::parse_target(operands[2], symbols, line_num)?;
                Ok(Instruction::Branch {
                    cond,
                    src1,
                    src2,
                    target,
                })
            }
            unknown => Err(ParseError {
                line_number: line_num,
                message: format!("Unknown instruction opcode '{}'", unknown),
            }),
        }
    }
}
