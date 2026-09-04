//! Unit test suite verifying core architectural invariants of Ω-Core.

use omega_vm::{run_source, TrapReason};

#[test]
fn test_destructive_reads() {
    // 1. Without @: r1 is cleared to 0 after consumption
    let src1 = r#"
        mov r1, 42
        add r0, r1, 10
        trap 0
    "#;
    let (state1, code1) = run_source(src1, 1000).expect("src1 failed");
    assert_eq!(code1, 0);
    assert_eq!(state1.r[0].to_u32(), 52);
    assert_eq!(state1.r[1].to_u32(), 0, "r1 must be cleared destructively");

    // 2. With @: r1 is preserved after consumption
    let src2 = r#"
        mov r1, 42
        add r0, @r1, 10
        trap 0
    "#;
    let (state2, code2) = run_source(src2, 1000).expect("src2 failed");
    assert_eq!(code2, 0);
    assert_eq!(state2.r[0].to_u32(), 52);
    assert_eq!(state2.r[1].to_u32(), 42, "r1 must be retained with @");

    // 3. Destructive self-overlap: add r1, r1, @r1
    // Evaluates first r1 (reads 42, clears r1 to 0).
    // Evaluates second @r1 (reads current r1 which is now 0).
    // Sum = 42 + 0 = 42 written to r1.
    let src3 = r#"
        mov r1, 42
        add r1, r1, @r1
        trap 0
    "#;
    let (state3, _) = run_source(src3, 1000).expect("src3 failed");
    assert_eq!(state3.r[1].to_u32(), 42);

    // 4. add r1, r1, r1 without @
    // Evaluates first r1 (reads 42, clears r1 to 0).
    // Evaluates second r1 (reads 0, clears r1 to 0).
    // Result = 42 + 0 = 42 written to r1.
    let src4 = r#"
        mov r1, 42
        add r1, r1, r1
        trap 0
    "#;
    let (state4, _) = run_source(src4, 1000).expect("src4 failed");
    assert_eq!(state4.r[1].to_u32(), 42);
}

#[test]
fn test_subslice_destructive_reads() {
    let src = r#"
        mov r1, 0x123456
        mov r0, r1.b0
        trap 0
    "#;
    let (state, _) = run_source(src, 1000).expect("subslice test failed");
    assert_eq!(state.r[0].to_u32(), 0x56);
    // Byte 0 should be cleared to 00, bytes 1 and 2 intact (0x123400)
    assert_eq!(state.r[1].to_u32(), 0x123400);
}

#[test]
fn test_24bit_modular_arithmetic() {
    let src = r#"
        mov r0, 0xFFFFFF
        add r0, @r0, 1
        trap 0
    "#;
    let (state, _) = run_source(src, 1000).expect("modular add failed");
    // 0xFFFFFF + 1 wraps to 0 in 24-bit ring
    assert_eq!(state.r[0].to_u32(), 0);
    assert!(state.flags.c, "Carry flag must be set on wrap");

    let src_sub = r#"
        mov r0, 0
        sub r0, @r0, 1
        trap 0
    "#;
    let (state_sub, _) = run_source(src_sub, 1000).expect("modular sub failed");
    // 0 - 1 wraps to 0xFFFFFF (-1 in 24-bit)
    assert_eq!(state_sub.r[0].to_u32(), 0xFFFFFF);
    assert_eq!(state_sub.r[0].to_i32(), -1);
}

#[test]
fn test_multiplication_and_division() {
    let src = r#"
        mov r1, 1000
        mov r2, 2500
        mul r0, @r1, @r2
        divs r3, @r0, @r1
        mods r4, @r0, @r1
        trap 0
    "#;
    let (state, _) = run_source(src, 1000).expect("mul/div failed");
    assert_eq!(state.r[0].to_u32(), 2_500_000);
    assert_eq!(state.r[3].to_u32(), 2500);
    assert_eq!(state.r[4].to_u32(), 0);
}

#[test]
fn test_division_by_zero_trap() {
    let src = r#"
        mov r1, 100
        divs r0, @r1, 0
        trap 0
    "#;
    let res = run_source(src, 1000);
    assert_eq!(res.unwrap_err(), TrapReason::DivZero);
}

#[test]
fn test_bit_reversal() {
    let src = r#"
        mov r1, 0b1
        rev r0, @r1
        rev r2, @r0
        trap 0
    "#;
    let (state, _) = run_source(src, 1000).expect("rev failed");
    // Bit 0 set reversed becomes Bit 23 set: 0x800000
    assert_eq!(state.r[0].to_u32(), 0x800000);
    // Double reversal returns original value
    assert_eq!(state.r[2].to_u32(), 0b1);
}

#[test]
fn test_asymmetric_bank_stalls() {
    // Accessing addr 0 and addr 7 in succession causes bank 0 collision stall
    let src_collision = r#"
        mov r1, 0
        mov r2, 7
        ldw r3, [r1]
        ldw r4, [r2]
        trap 0
    "#;
    let (state_col, _) = run_source(src_collision, 1000).expect("bank collision failed");
    assert!(
        state_col.metrics.bank_stall_cycles > 0,
        "Consecutive accesses to same bank (0 and 7 mod 7) must incur stall cycles"
    );

    // Accessing addr 0 and addr 1 causes NO bank collision stall
    let src_no_collision = r#"
        mov r1, 0
        mov r2, 1
        ldw r3, [r1]
        ldw r4, [r2]
        trap 0
    "#;
    let (state_no_col, _) = run_source(src_no_collision, 1000).expect("bank independent failed");
    assert_eq!(
        state_no_col.metrics.bank_stall_cycles, 0,
        "Access to bank 0 followed by bank 1 must have 0 stall cycles"
    );
}

#[test]
fn test_ring_buffer_call_ret() {
    let src = r#"
        mov r0, 0
        call func_a
        trap 0

    func_a:
        add r0, @r0, 10
        call func_b
        add r0, @r0, 1
        ret

    func_b:
        add r0, @r0, 100
        ret
    "#;
    let (state, _) = run_source(src, 1000).expect("call/ret failed");
    // r0 should be 0 + 10 + 100 + 1 = 111
    assert_eq!(state.r[0].to_u32(), 111);
    assert_eq!(state.metrics.call_instructions, 2);
    assert_eq!(state.metrics.ret_instructions, 2);
}

#[test]
fn test_ring_buffer_save_restore() {
    let src = r#"
        // Save current ring buffer to memory at address 0x2000
        mov r1, 0x2000
        rbsave [r1]
        // Modify and restore
        rbrst [r1]
        trap 0
    "#;
    let (state, _) = run_source(src, 1000).expect("rbsave/rbrst failed");
    assert!(state.metrics.total_cycles > 16);
}

#[test]
fn test_memory_out_of_bounds_trap() {
    let src = r#"
        mov r1, 0x10000 // 65536 is >= 64K
        ldw r0, [r1]
        trap 0
    "#;
    let res = run_source(src, 1000);
    assert_eq!(res.unwrap_err(), TrapReason::AddrOutOfBounds(65536));
}
