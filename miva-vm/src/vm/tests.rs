use super::*;
use crate::host::{MivaValue, HostFn};

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
        functions: vec![
            MvmFunction { name_idx: 0, arity: 0, locals: 0, is_async: false, code },
        ],
    };

    let mut vm = Mvm::new(program);
    vm.register_host("add_one", host_add_one as HostFn);
    vm.run().unwrap();

    // Result of RetVal is left on the stack; pop it.
    let result = vm.stack.pop().expect("result on stack");
    assert_eq!(result, Value::Int(42));
}
