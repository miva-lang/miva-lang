use super::*;
use crate::host::{HostFn, MivaValue};

// Host fn: returns args[0] + 1.
unsafe extern "C" fn host_add_one(args: *const MivaValue, _argc: i32) -> MivaValue {
    let a = &*args;
    MivaValue::int(a.data.i + 1)
}

#[test]
fn call_host_registered_fn() {
    // main: push 41 -> CallHost("add_one", arity=1) -> RetVal
    let mut code = Vec::new();
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&41i64.to_le_bytes());
    code.push(Opcode::CallHost as u8);
    // name string index = 0 ("add_one")
    code.extend_from_slice(&0u32.to_le_bytes());
    code.push(1u8); // arity
    code.push(Opcode::RetVal as u8);

    let program = MvmProgram {
        strings: vec!["add_one".to_string()],
        functions: vec![MvmFunction {
            name_idx: 0,
            arity: 0,
            locals: 0,
            is_async: false,
            code,
        }],
    };

    let mut vm = Mvm::new(program);
    vm.register_host("add_one", host_add_one as HostFn);
    vm.run().unwrap();

    // Result of RetVal is left on the stack; pop it.
    let result = vm.stack.pop().expect("result on stack");
    assert_eq!(result, Value::Int(42));
}

use crate::error::TrapKind;

fn program_with_main(code: Vec<u8>) -> MvmProgram {
    MvmProgram {
        strings: vec!["main".to_string()],
        functions: vec![MvmFunction {
            name_idx: 0,
            arity: 0,
            locals: 0,
            is_async: false,
            code,
        }],
    }
}

fn run_expect_trap(code: Vec<u8>) -> VmError {
    let mut vm = Mvm::new(program_with_main(code));
    vm.run().expect_err("expected a VM trap")
}

#[test]
fn to_bytes_from_bytes_round_trip() {
    let program = MvmProgram {
        strings: vec!["main".to_string(), "hello".to_string()],
        functions: vec![
            MvmFunction {
                name_idx: 0,
                arity: 0,
                locals: 2,
                is_async: false,
                code: vec![1, 2, 3],
            },
            MvmFunction {
                name_idx: 1,
                arity: 3,
                locals: 5,
                is_async: true,
                code: vec![],
            },
        ],
    };
    let bytes = program.to_bytes();
    let loaded = MvmProgram::from_bytes(&bytes).unwrap();
    assert_eq!(loaded.strings, program.strings);
    assert_eq!(loaded.functions.len(), 2);
    assert_eq!(loaded.functions[0].code, vec![1, 2, 3]);
    assert_eq!(loaded.functions[1].arity, 3);
    assert!(loaded.functions[1].is_async);
}

#[test]
fn from_bytes_rejects_bad_magic() {
    let err = MvmProgram::from_bytes(b"XXXX").unwrap_err();
    assert_eq!(err.kind, TrapKind::InvalidBytecode);
}

#[test]
fn from_bytes_rejects_truncated_data() {
    let program = program_with_main(vec![Opcode::Halt as u8]);
    let bytes = program.to_bytes();
    // Every strict prefix must be rejected without panicking.
    for len in 0..bytes.len() {
        let err = MvmProgram::from_bytes(&bytes[..len]).unwrap_err();
        assert_eq!(err.kind, TrapKind::InvalidBytecode, "prefix len {}", len);
    }
}

#[test]
fn from_bytes_rejects_bad_name_index() {
    let program = MvmProgram {
        strings: vec!["main".to_string()],
        functions: vec![MvmFunction {
            name_idx: 9,
            arity: 0,
            locals: 0,
            is_async: false,
            code: vec![],
        }],
    };
    let err = MvmProgram::from_bytes(&program.to_bytes()).unwrap_err();
    assert_eq!(err.kind, TrapKind::InvalidBytecode);
}

#[test]
fn unknown_opcode_traps() {
    let err = run_expect_trap(vec![0xFF]);
    assert_eq!(err.kind, TrapKind::Runtime);
    assert!(err.message.contains("Unknown opcode"));
}

#[test]
fn truncated_operand_traps() {
    // PushI64 expects 8 operand bytes; provide none.
    let err = run_expect_trap(vec![Opcode::PushI64 as u8]);
    assert_eq!(err.kind, TrapKind::InvalidBytecode);
}

#[test]
fn stack_underflow_traps() {
    let err = run_expect_trap(vec![Opcode::Drop as u8]);
    assert_eq!(err.kind, TrapKind::StackUnderflow);
}

#[test]
fn division_by_zero_traps() {
    let mut code = Vec::new();
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&1i64.to_le_bytes());
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&0i64.to_le_bytes());
    code.push(Opcode::I64Div as u8);
    let err = run_expect_trap(code);
    assert_eq!(err.kind, TrapKind::DivisionByZero);
}

#[test]
fn remainder_by_zero_traps() {
    let mut code = Vec::new();
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&1i64.to_le_bytes());
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&0i64.to_le_bytes());
    code.push(Opcode::I64Rem as u8);
    let err = run_expect_trap(code);
    assert_eq!(err.kind, TrapKind::DivisionByZero);
}

#[test]
fn call_out_of_bounds_function_traps() {
    let mut code = Vec::new();
    code.push(Opcode::Call as u8);
    code.extend_from_slice(&99u32.to_le_bytes());
    let err = run_expect_trap(code);
    assert_eq!(err.kind, TrapKind::InvalidBytecode);
}

#[test]
fn deep_recursion_traps_instead_of_native_overflow() {
    // main calls itself forever: Call 0.
    let mut code = Vec::new();
    code.push(Opcode::Call as u8);
    code.extend_from_slice(&0u32.to_le_bytes());
    let vm = Mvm::new(program_with_main(code));
    let err = vm.run_on_vm_thread().expect_err("expected a VM trap");
    assert_eq!(err.kind, TrapKind::StackOverflow);
}

#[test]
fn array_get_out_of_bounds_traps() {
    let mut code = Vec::new();
    // Build an empty mutable array: push count 0, ArrayNew.
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&0i64.to_le_bytes());
    code.push(Opcode::ArrayNew as u8);
    // Index 5 into it.
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&5i64.to_le_bytes());
    code.push(Opcode::ArrayGet as u8);
    let err = run_expect_trap(code);
    assert_eq!(err.kind, TrapKind::OutOfBounds);
}

#[test]
fn simple_arithmetic_runs() {
    let mut code = Vec::new();
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&40i64.to_le_bytes());
    code.push(Opcode::PushI64 as u8);
    code.extend_from_slice(&2i64.to_le_bytes());
    code.push(Opcode::I64Add as u8);
    code.push(Opcode::Exit as u8);
    let mut vm = Mvm::new(program_with_main(code));
    assert_eq!(vm.run().unwrap(), 42);
}
