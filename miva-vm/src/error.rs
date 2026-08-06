use std::fmt;

/// Category of a VM trap. Lets embedders distinguish malformed bytecode
/// from runtime faults without parsing the message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapKind {
    InvalidBytecode,
    StackOverflow,
    StackUnderflow,
    DivisionByZero,
    TypeError,
    OutOfBounds,
    Runtime,
}

impl TrapKind {
    pub fn code(self) -> &'static str {
        match self {
            TrapKind::InvalidBytecode => "VM_INVALID_BYTECODE",
            TrapKind::StackOverflow => "VM_STACK_OVERFLOW",
            TrapKind::StackUnderflow => "VM_STACK_UNDERFLOW",
            TrapKind::DivisionByZero => "VM_DIVISION_BY_ZERO",
            TrapKind::TypeError => "VM_TYPE_ERROR",
            TrapKind::OutOfBounds => "VM_OUT_OF_BOUNDS",
            TrapKind::Runtime => "VM_RUNTIME_ERROR",
        }
    }
}

/// Structured VM error: a trap kind plus a human-readable message.
#[derive(Debug, Clone)]
pub struct VmError {
    pub kind: TrapKind,
    pub message: String,
}

impl VmError {
    pub fn new(kind: TrapKind, message: impl Into<String>) -> Self {
        VmError {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn invalid_bytecode(message: impl Into<String>) -> Self {
        Self::new(TrapKind::InvalidBytecode, message)
    }

    pub(crate) fn stack_overflow() -> Self {
        Self::new(TrapKind::StackOverflow, "MVM stack overflow")
    }

    pub(crate) fn stack_underflow() -> Self {
        Self::new(TrapKind::StackUnderflow, "MVM stack underflow")
    }

    pub(crate) fn division_by_zero() -> Self {
        Self::new(TrapKind::DivisionByZero, "division by zero")
    }

    pub(crate) fn expected_int() -> Self {
        Self::new(TrapKind::TypeError, "expected int operand")
    }

    pub(crate) fn expected_float() -> Self {
        Self::new(TrapKind::TypeError, "expected float operand")
    }

    pub(crate) fn out_of_bounds(message: impl Into<String>) -> Self {
        Self::new(TrapKind::OutOfBounds, message)
    }
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for VmError {}

impl From<String> for VmError {
    fn from(message: String) -> Self {
        Self::new(TrapKind::Runtime, message)
    }
}

impl From<&str> for VmError {
    fn from(message: &str) -> Self {
        Self::new(TrapKind::Runtime, message)
    }
}
