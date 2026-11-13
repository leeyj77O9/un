//! `vm2` — a minimal register-based VM used to measure hot-loop cost.
//!
//! Phase 2a: only i64 scalar ops and nested-loop control flow, with programs
//! built by an inline builder (no parser yet). A correctness harness runs a
//! single benchmark against the stack VM's implementation and reports both.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    LoadConst = 0,
    Move = 1,
    Add = 2,
    Sub = 3,
    Mul = 4,
    Lt = 5,
    Ge = 6,
    Jump = 7,
    JumpIfFalse = 8,
    Return = 9,
}

#[derive(Debug, Clone)]
pub struct Ins {
    op: Op,
    dst: u8,
    src1: u8,
    src2: u8,
    imm: i64,
}

pub struct Program {
    pub code: Vec<Ins>,
    pub registers: u8,
}

impl Program {
    pub fn new() -> Self {
        Self { code: Vec::new(), registers: 0 }
    }

    pub fn emit(&mut self, op: Op, dst: u8, src1: u8, src2: u8, imm: i64) -> usize {
        assert!(dst < 32 && src1 < 32 && src2 < 32, "register index out of range");
        if dst >= self.registers { self.registers = dst + 1; }
        if src1 >= self.registers { self.registers = src1 + 1; }
        if src2 >= self.registers { self.registers = src2 + 1; }
        self.code.push(Ins { op, dst, src1, src2, imm });
        self.code.len() - 1
    }

    fn patch_jump(&mut self, ip: usize, target: usize) {
        assert!(ip < self.code.len(), "patch_jump out of bounds");
        self.code[ip].imm = target as i64;
    }

    /// fn nested_sum(i64 size) -> i64:
    ///     total = 0
    ///     i = 1
    ///     loop while i <= size:
    ///         j = 1
    ///         loop while j <= size:
    ///             total = total + i * j
    ///             j = j + 1
    ///         i = i + 1
    ///     return total
    pub fn nested_sum(size: i64) -> i64 {
        let mut p = Self::new();
        let total = 0u8;
        let i = 1u8;
        let j = 2u8;
        let n = 3u8;
        let tmp = 4u8;

        p.emit(Op::LoadConst, n, 0, 0, size);
        p.emit(Op::LoadConst, total, 0, 0, 0);
        p.emit(Op::LoadConst, i, 0, 0, 1);

        let outer = p.code.len();
        p.emit(Op::Ge, tmp, n, i, 0);
        let outer_exit = p.emit(Op::JumpIfFalse, tmp, 0, 0, 0);

        p.emit(Op::LoadConst, j, 0, 0, 1);
        let inner = p.code.len();
        p.emit(Op::Ge, tmp, n, j, 0);
        let inner_exit = p.emit(Op::JumpIfFalse, tmp, 0, 0, 0);

        p.emit(Op::Mul, tmp, i, j, 0);
        p.emit(Op::Add, total, total, tmp, 0);

        p.emit(Op::LoadConst, tmp, 0, 0, 1);
        p.emit(Op::Add, j, j, tmp, 0);
        p.emit(Op::Jump, 0, 0, 0, inner as i64);

        let next_i = p.code.len();
        p.emit(Op::LoadConst, tmp, 0, 0, 1);
        p.emit(Op::Add, i, i, tmp, 0);
        p.emit(Op::Jump, 0, 0, 0, outer as i64);

        let exit = p.code.len();
        p.emit(Op::Return, total, 0, 0, 0);

        p.patch_jump(outer_exit, exit);
        p.patch_jump(inner_exit, next_i);

        let r = Self::run(&p);
        r
    }

    pub fn run(&self) -> i64 {
        let mut regs = vec![0i64; 32];
        let mut ip = 0usize;
        loop {
            if ip >= self.code.len() {
                return 0;
            }
            let ins = &self.code[ip];
            match ins.op {
                Op::LoadConst => {
                    regs[ins.dst as usize] = ins.imm;
                    ip += 1;
                }
                Op::Move => {
                    regs[ins.dst as usize] = regs[ins.src1 as usize];
                    ip += 1;
                }
                Op::Add => {
                    regs[ins.dst as usize] = regs[ins.src1 as usize].wrapping_add(regs[ins.src2 as usize]);
                    ip += 1;
                }
                Op::Sub => {
                    regs[ins.dst as usize] = regs[ins.src1 as usize].wrapping_sub(regs[ins.src2 as usize]);
                    ip += 1;
                }
                Op::Mul => {
                    regs[ins.dst as usize] = regs[ins.src1 as usize].wrapping_mul(regs[ins.src2 as usize]);
                    ip += 1;
                }
                Op::Lt => {
                    regs[ins.dst as usize] = if regs[ins.src1 as usize] < regs[ins.src2 as usize] { 1 } else { 0 };
                    ip += 1;
                }
                Op::Ge => {
                    regs[ins.dst as usize] = if regs[ins.src1 as usize] >= regs[ins.src2 as usize] { 1 } else { 0 };
                    ip += 1;
                }
                Op::Jump => {
                    ip = ins.imm as usize;
                }
                Op::JumpIfFalse => {
                    if regs[ins.dst as usize] == 0 {
                        ip = ins.imm as usize;
                    } else {
                        ip += 1;
                    }
                }
                Op::Return => {
                    return regs[ins.dst as usize];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_sum_small() {
        assert_eq!(Program::nested_sum(0), 0);
        assert_eq!(Program::nested_sum(1), 1);
        assert_eq!(Program::nested_sum(2), 9);
        assert_eq!(Program::nested_sum(3), 36);
    }

    #[test]
    fn nested_sum_200() {
        assert_eq!(Program::nested_sum(200), 404010000);
    }

    #[test]
    fn bench_register_vm() {
        use std::time::Instant;
        let start = Instant::now();
        let mut acc: i64 = 0;
        for _ in 0..20 {
            acc = acc.wrapping_add(Program::nested_sum(200));
        }
        let elapsed = start.elapsed();
        assert_eq!(acc, 404010000 * 20);
        println!("register vm2 nested_sum(200) x20: {:?}", elapsed);
    }
}
