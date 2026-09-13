//! [nb:core] Program-independent RISC-V ABI declarations and their admission rules.
//! Builders and JSON ingress establish one immutable declaration; source matching stays in Shipyard.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::collections::BTreeSet;

/// Architectural register identity; ABI admission validates the 32-register domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RiscvRegister(pub u8);
impl RiscvRegister { pub fn number(self) -> u8 { self.0 } }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
/// Stable, caller-owned identity of one complete ABI declaration.
pub struct AbiDialectId(pub String);

impl From<&str> for AbiDialectId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for AbiDialectId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
/// Stable, caller-owned identity of one signal within an ABI declaration.
pub struct AbiSignalId(pub String);

impl From<&str> for AbiSignalId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for AbiSignalId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Identifies a parameter within an ABI signal.
///
/// Different signals may use the same parameter identifier. [`AbiParamRef`] combines it
/// with the signal's identity to refer to one declared parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AbiParamId(pub u16);

/// Defines how ABI calls reach the host.
///
/// This selects the call mechanism. The host handler is supplied separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiTransport {
    /// Calls the host handler through the Vehicle's foreign-function interface (FFI).
    GenericHostFfi,
}

/// Defines the source of a value written to a guest register by an ABI operation.
///
/// Used in [`AbiWriteDefinition`], which specifies the destination register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiWriteValue {
    /// The result word returned by the host handler.
    HostReturnWord,
}

/// Values that a supplied ABI may place in architectural registers before guest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiEntryValue {
    /// Inclusive guest address of the first allocatable heap byte.
    GuestHeapBase,
    /// Number of allocatable bytes before the guard range.
    GuestHeapLength,
}

/// Defines which guest registers receive the heap's base address and byte length.
///
/// Both values are supplied before the guest's first instruction executes. The base is
/// a guest address, not a host pointer. The guest can use this pair to initialize its allocator.
///
/// The registers must be distinct. Neither may be `x0` or the stack pointer (`x2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiGuestHeapRegisters {
    /// The register that receives the heap's base address.
    base: RiscvRegister,
    /// The register that receives the heap's byte length.
    length: RiscvRegister,
}

impl AbiGuestHeapRegisters {
    pub const fn base(self) -> RiscvRegister {
        self.base
    }

    pub const fn length(self) -> RiscvRegister {
        self.length
    }
}

/// Declares the values supplied through guest registers at program entry.
///
/// These values are established before the guest's first instruction executes. They are
/// available to startup code without an ABI call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiEntryContract {
    /// The optional register pair for delivering the guest heap.
    ///
    /// `None` leaves heap delivery undeclared; it does not disable the heap.
    guest_heap: Option<AbiGuestHeapRegisters>,
}

impl AbiEntryContract {
    pub const fn guest_heap(self) -> Option<AbiGuestHeapRegisters> {
        self.guest_heap
    }

    /// Returns the ABI entry value assigned to `register`, if any.
    pub fn value_for_register(self, register: RiscvRegister) -> Option<AbiEntryValue> {
        let heap = self.guest_heap?;
        if register == heap.base {
            return Some(AbiEntryValue::GuestHeapBase);
        }
        if register == heap.length {
            return Some(AbiEntryValue::GuestHeapLength);
        }
        None
    }

    /// Returns the architectural register selected for `value`, if it is declared.
    pub fn register_for_value(self, value: AbiEntryValue) -> Option<RiscvRegister> {
        let heap = self.guest_heap?;
        Some(match value {
            AbiEntryValue::GuestHeapBase => heap.base,
            AbiEntryValue::GuestHeapLength => heap.length,
        })
    }
}

/// Declares the ABI that a guest RISC-V program follows.
///
/// This declaration tells the compiler the effects of ABI calls, such as, whether
/// a call returns and what registers it writes.
///
/// The compiler uses ABI signals to identify guest instructions that request ABI operations. The
/// signal describes the pattern that is applied to every instruction. Matched instructions are
/// translated according to the declared behavior. One signal can serve several operations.
///
/// The ABI is constructed with [`RiscvAbi::builder`]. This only declares the ABI behaviour. The
/// host handler is supplied separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AbiDeclaration")]
pub struct RiscvAbi {
    /// The identity of this ABI.
    id: AbiDialectId,
    /// The transport used for guest-to-host calls.
    transport: AbiTransport,
    /// The ABI's program-entry contract.
    entry: AbiEntryContract,
    /// The signals declared by this ABI.
    signals: Vec<AbiSignalDefinition>,
}

impl RiscvAbi {
    /// Starts a validating declaration builder.
    pub fn builder(id: impl Into<AbiDialectId>) -> RiscvAbiBuilder {
        RiscvAbiBuilder::new(id)
    }

    /// Returns the identity retained in binding and harness evidence.
    pub fn id(&self) -> &AbiDialectId {
        &self.id
    }

    /// Immutable declarations consumed by source-specific instruction matching.
    pub fn signals(&self) -> &[AbiSignalDefinition] { &self.signals }

    /// Returns the declaration's selected guest-host transport.
    pub fn transport(&self) -> AbiTransport {
        self.transport
    }

    /// Returns the values supplied before the effective ELF entry executes.
    pub fn entry(&self) -> AbiEntryContract {
        self.entry
    }
}

#[derive(Debug, Clone)]
/// Validating constructor for one immutable [`RiscvAbi`] declaration.
pub struct RiscvAbiBuilder {
    id: AbiDialectId,
    transport: Option<AbiTransport>,
    entry: AbiEntryContract,
    signals: Vec<AbiSignalDefinition>,
}

impl RiscvAbiBuilder {
    /// Starts an ABI declaration with a caller-owned identity.
    pub fn new(id: impl Into<AbiDialectId>) -> Self {
        Self {
            id: id.into(),
            transport: None,
            entry: AbiEntryContract::default(),
            signals: Vec::new(),
        }
    }

    /// Selects the transport available to every signal in this declaration.
    pub fn transport(mut self, transport: AbiTransport) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Supplies the complete guest-heap range through two program-entry registers.
    pub fn guest_heap_registers(
        mut self,
        base: RiscvRegister,
        length: RiscvRegister,
    ) -> Self {
        self.entry.guest_heap = Some(AbiGuestHeapRegisters { base, length });
        self
    }

    /// Adds one signal definition to the declaration.
    pub fn signal(mut self, signal: AbiSignalDefinition) -> Self {
        self.signals.push(signal);
        self
    }

    /// Validates the complete declaration and returns its immutable frontend form.
    pub fn build(self) -> Result<RiscvAbi, RiscvAbiBuildError> {
        validate_identity("ABI", &self.id.0)?;
        let transport = self.transport.ok_or(RiscvAbiBuildError::MissingTransport)?;
        validate_entry(self.entry)?;
        let mut signal_ids = BTreeSet::new();
        for signal in &self.signals {
            validate_signal(signal, transport)?;
            if !signal_ids.insert(signal.id.clone()) {
                return Err(RiscvAbiBuildError::DuplicateSignal {
                    signal: signal.id.clone(),
                });
            }
        }

        Ok(RiscvAbi {
            id: self.id,
            transport,
            entry: self.entry,
            signals: self.signals,
        })
    }
}

/// Defines an ABI signal that identifies guest instructions requesting ABI operations.
///
/// The compiler checks each instruction against the signal's pattern. For a matching instruction,
/// the signal's parameters and behaviour-selection rule determine how the compiler interprets it.
/// One signal can serve several operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiSignalDefinition {
    /// The signal's identity, unique within the owning ABI.
    pub id: AbiSignalId,
    /// The instruction pattern that identifies this signal.
    pub pattern: AbiSignalPattern,
    /// The parameters declared by this signal.
    pub params: Vec<AbiParamDefinition>,
    /// The rule for selecting an operation for a matched instruction.
    pub behavior: AbiBehaviorSelectionDefinition,
}

/// Constructs an ABI signal.
///
/// The builder starts with a signal identity and an instruction pattern. Parameters are added
/// with [`Self::param`], which defines where each parameter comes from and returns a reference
/// to it. These references are used to define the signal's selector and operation behaviours.
///
/// [`Self::build`] completes the signal with its behaviour-selection rule. The resulting
/// definition can be added to an ABI with [`RiscvAbiBuilder::signal`]. Validation happens when
/// the complete ABI is built.
#[derive(Debug, Clone)]
pub struct AbiSignalBuilder {
    /// The identity of this signal within the ABI.
    id: AbiSignalId,
    /// The instruction pattern that identifies this signal.
    pattern: AbiSignalPattern,
    /// The parameters declared for this signal.
    params: Vec<AbiParamDefinition>,
    /// The identifier assigned to the next declared parameter.
    next_param_id: u16,
}

impl AbiSignalBuilder {
    /// Starts a signal declaration for one supported instruction pattern.
    pub fn new(id: impl Into<AbiSignalId>, pattern: AbiSignalPattern) -> Self {
        Self {
            id: id.into(),
            pattern,
            params: Vec::new(),
            next_param_id: 0,
        }
    }

    /// Declares one signal-local parameter and returns its opaque reference.
    pub fn param(
        &mut self,
        name: impl Into<String>,
        source: AbiParamDefinitionSource,
    ) -> AbiParamRef {
        let reference = AbiParamRef {
            signal: self.id.clone(),
            id: AbiParamId(self.next_param_id),
        };
        self.next_param_id = self
            .next_param_id
            .checked_add(1)
            .expect("ABI signal parameter id overflow");
        self.params.push(AbiParamDefinition {
            reference: reference.clone(),
            name: name.into(),
            source,
        });
        reference
    }

    /// Completes the signal definition with its behavior-selection rule.
    pub fn build(self, behavior: AbiBehaviorSelectionDefinition) -> AbiSignalDefinition {
        AbiSignalDefinition {
            id: self.id,
            pattern: self.pattern,
            params: self.params,
            behavior,
        }
    }
}

/// A reference to a declared parameter in a signal. Can only be used within the signal that
/// declares the parameter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiParamRef {
    /// Signal that owns the parameter.
    signal: AbiSignalId,
    /// The parameter within that signal.
    id: AbiParamId,
}

/// Declares a parameter of an ABI signal.
///
/// The signal's selector and operation definitions refer to this parameter through
/// [`AbiParamRef`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiParamDefinition {
    /// The reference that identifies this parameter.
    pub reference: AbiParamRef,
    /// The parameter's name, unique within the owning ABI signal.
    pub name: String,
    /// The source of this parameter.
    pub source: AbiParamDefinitionSource,
}

/// Defines which guest instructions identify an ABI signal.
///
/// The pattern matches the instruction and its operands. It does not define the operation's
/// behaviour or check values held in guest registers. Those are declared separately through
/// the signal's parameters and behaviour-selection rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiSignalPattern {
    /// Matches an `ecall` instruction.
    Ecall,

    /// Matches `csrrw rd, csr, rs1` with the specified CSR address and register conditions.
    ///
    /// Register conditions apply to the register numbers, not the values they hold.
    Csrrw {
        /// The control and status register (CSR) address to match.
        ///
        /// Must be in the range `0x000..=0xfff`.
        csr: u16,
        /// The constraint on the instruction's destination register.
        rd: RegisterMatcher,
        /// The constraint on the instruction's source register.
        rs1: RegisterMatcher,
    },
}

/// A matching constraint on a register operand.
///
/// The constraint applies to the register number, not the value held in the register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RegisterMatcher {
    /// Accepts any register, including `x0`.
    Any,
    /// Accepts any register except `x0`.
    AnyNonZero,
    /// Accepts only the specified register.
    Exact(RiscvRegister),
}

/// Defines where an ABI parameter comes from.
///
/// This is used by the compiler to wire the ABI parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiParamDefinitionSource {
    /// The value held in the specified guest register.
    ///
    /// Intended for cases where the ABI names registers that are not encoded in the instruction,
    /// such as `ecall`.
    Register(RiscvRegister),

    /// The value held in the register named by the instruction's `rs1` operand.
    OperandRs1Value,

    /// The destination register named by the instruction's `rd` operand.
    OperandRdRegister,

    /// A constant supplied by this ABI declaration, not extracted from the instruction.
    Immediate(u32),

    /// The matched instruction's complete machine-code encoding.
    RawInstructionWord,

    /// The guest address of the matched RISC-V instruction.
    ///
    /// This is not an address in the compiled native Vehicle.
    SourcePc,
}

/// Defines how an ABI selects an operation.
///
/// Use a selector table when one instruction, such as `ecall`, serves several operations. Use a
/// fixed definition when the instruction always requests the same operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiBehaviorSelectionDefinition {
    /// Selects an operation by matching a guest register's value against provided cases.
    SelectorTable {
        /// The parameter whose register holds the selector value.
        ///
        /// The parameter reference must refer to parameter declared by the owning ABI
        /// signal.
        /// Set the selector register to a constant. There must be no control-flow
        /// instruction between that assignment and the ABI call.
        ///
        /// Example:
        /// ```asm
        /// li a7, 1
        /// ecall
        /// ```
        selector: AbiParamRef,
        /// The cases available to this signal.
        cases: Vec<AbiSelectorCaseDefinition>,
        /// The policy for selector values with no matching case.
        unknown_policy: AbiUnknownSelectorPolicy,
    },
    /// Uses one operation definition for every match.
    ///
    /// Calls may request different client operations. Their compiler-visible
    /// effects are the same, so the compiler does not need a selector to
    /// distinguish them.
    Fixed(AbiBehaviorDefinition),
}

/// Describes one ABI operation.
///
/// Used in [`AbiBehaviorSelectionDefinition::SelectorTable`], which defines the selector register.
/// Each case describes one supported value of the selector register.
///
/// For example: an ABI can use `a0 = 1; ecall;` to read input length. The case with selector `1`
/// describes that operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiSelectorCaseDefinition {
    /// The value that selects this case.
    ///
    /// This is the value held in the selector register, not its register number.
    /// Values must be unique within the containing selector table.
    pub selector: u32,
    /// The definition of this case's operation.
    ///
    /// Parameter references must refer to parameters declared by the owning ABI
    /// signal.
    pub behavior: AbiBehaviorDefinition,
}

/// Defines what happens when a selector value has no matching case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiUnknownSelectorPolicy {
    /// Rejects compilation of the guest program.
    Reject,
}

/// Declares the compiler-visible effects of an ABI operation.
///
/// The compiler uses this definition to preserve the operation's inputs, account for
/// register writes, and determine whether guest execution continues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiBehaviorDefinition {
    /// The ordered parameters that this operation receives.
    pub required_params: Vec<AbiParamRef>,
    /// The guest register writes performed by this operation.
    pub writes: Vec<AbiWriteDefinition>,
    /// The operation's effect on guest control flow.
    pub control: AbiControlDefinition,
}

/// Defines whether an ABI operation continues or ends Vehicle execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiControlDefinition {
    /// Continues at the next guest instruction after the host call returns.
    Continue,

    /// Calls the host handler, then ends Vehicle execution with a trap.
    ///
    /// The handler's result word is discarded.
    TerminatesTrap,

    /// Ends Vehicle execution successfully and publishes its output.
    TerminatesComplete {
        /// The definition of the completed execution's output.
        output: AbiOutputDefinition,
    },
}

/// Declares a guest register write performed by an ABI operation.
///
/// The value comes from the declared source and replaces the destination register's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiWriteDefinition {
    /// The destination guest register.
    ///
    /// Must not be `x0`. Each register may appear only once in an operation's writes.
    pub dst: RiscvRegister,
    /// The source of the value to write.
    pub value: AbiWriteValue,
}

/// Defines how the Vehicle publishes its output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiOutputDefinition {
    /// Publishes a guest-memory slice as the output.
    ///
    /// Both parameters must appear in the operation's required parameters.
    Slice {
        /// The parameter that supplies the slice's guest address, not a host pointer.
        ptr: AbiParamRef,
        /// The parameter that supplies the slice's byte length.
        len: AbiParamRef,
    },
}


#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum RiscvAbiBuildError {
    #[error("architectural register x{register} is outside x0..x31")]
    InvalidRegister { register: u8 },
    #[error("CSR selector {csr} exceeds its 12-bit encoding")]
    InvalidCsr { csr: u16 },
    #[error("ABI signal {signal:?} declares an operand unavailable on its instruction")]
    UnavailableOperand { signal: AbiSignalId },
    #[error("ABI signal {signal:?} contains duplicate parameter name {name}")]
    DuplicateParamName { signal: AbiSignalId, name: String },
    #[error("{kind} identity must not be empty")]
    EmptyIdentity { kind: &'static str },
    #[error("RISC-V ABI declaration has no transport")]
    MissingTransport,
    #[error("RISC-V ABI entry value {value:?} cannot use architectural zero register x0")]
    ZeroEntryRegister { value: AbiEntryValue },
    #[error("RISC-V ABI entry value {value:?} cannot replace the layout-owned stack pointer x2")]
    StackPointerEntryRegister { value: AbiEntryValue },
    #[error(
        "RISC-V ABI entry values {first:?} and {second:?} both use architectural register x{register}"
    )]
    DuplicateEntryRegister {
        first: AbiEntryValue,
        second: AbiEntryValue,
        register: u8,
    },
    #[error("RISC-V ABI declaration contains duplicate signal {signal:?}")]
    DuplicateSignal { signal: AbiSignalId },
    #[error("RISC-V ABI signal {signal:?} contains duplicate parameter id {param:?}")]
    DuplicateParam {
        signal: AbiSignalId,
        param: AbiParamId,
    },
    #[error("RISC-V ABI signal {signal:?} contains duplicate selector 0x{selector:08x}")]
    DuplicateSelector { signal: AbiSignalId, selector: u32 },
    #[error("RISC-V ABI signal {signal:?} contains duplicate required parameter {param:?}")]
    DuplicateRequiredParam {
        signal: AbiSignalId,
        param: AbiParamId,
    },
    #[error("RISC-V ABI signal {signal:?} contains duplicate write to register x{register}")]
    DuplicateWrite {
        signal: AbiSignalId,
        register: u8,
    },
    #[error("RISC-V ABI signal {signal:?} references undeclared parameter {param:?}")]
    UndeclaredParam {
        signal: AbiSignalId,
        param: AbiParamId,
    },
    #[error("RISC-V ABI signal {signal:?} selector parameter {param:?} is not register-backed")]
    NonRegisterSelector {
        signal: AbiSignalId,
        param: AbiParamId,
    },
    #[error("RISC-V ABI signal {signal:?} completion output parameter {param:?} is not declared as a required input")]
    OutputParamNotRequired {
        signal: AbiSignalId,
        param: AbiParamId,
    },
    #[error("RISC-V ABI signal {signal:?} declares a write to architectural zero register x0")]
    ZeroRegisterWrite { signal: AbiSignalId },
}

pub mod dialects {
    use super::*;

    /// Demo transport declaration; services are implemented by the client.
    pub fn demo_dialect() -> RiscvAbi {
        let mut ecall = AbiSignalBuilder::new("demo.ecall", AbiSignalPattern::Ecall);
        let selector = ecall.param(
            "selector",
            AbiParamDefinitionSource::Register(RiscvRegister(17)),
        );
        let arg0 = ecall.param(
            "arg0",
            AbiParamDefinitionSource::Register(RiscvRegister(10)),
        );
        let arg1 = ecall.param(
            "arg1",
            AbiParamDefinitionSource::Register(RiscvRegister(11)),
        );
        let arg2 = ecall.param(
            "arg2",
            AbiParamDefinitionSource::Register(RiscvRegister(12)),
        );
        ecall.param("raw", AbiParamDefinitionSource::RawInstructionWord);
        ecall.param("pc", AbiParamDefinitionSource::SourcePc);

        let signal = ecall.build(AbiBehaviorSelectionDefinition::SelectorTable {
            selector,
            cases: vec![
                selector_case_continue(0x01, Vec::new()),
                selector_case_continue(0x02, vec![arg0.clone()]),
                AbiSelectorCaseDefinition {
                    selector: 0x03,
                    behavior: AbiBehaviorDefinition {
                        required_params: Vec::new(),
                        writes: Vec::new(),
                        control: AbiControlDefinition::TerminatesTrap,
                    },
                },
                AbiSelectorCaseDefinition {
                    selector: 0x04,
                    behavior: AbiBehaviorDefinition {
                        required_params: vec![arg0.clone(), arg1.clone()],
                        writes: Vec::new(),
                        control: AbiControlDefinition::TerminatesComplete {
                            output: AbiOutputDefinition::Slice {
                                ptr: arg0.clone(),
                                len: arg1.clone(),
                            },
                        },
                    },
                },
                selector_case_continue(
                    0x05,
                    vec![arg0.clone(), arg1.clone(), arg2.clone()],
                ),
                selector_case_continue(0x06, vec![arg0, arg1, arg2]),
            ],
            unknown_policy: AbiUnknownSelectorPolicy::Reject,
        });

        RiscvAbi::builder("demo_dialect")
            .transport(AbiTransport::GenericHostFfi)
            .guest_heap_registers(
                RiscvRegister(10),
                RiscvRegister(11),
            )
            .signal(signal)
            .build()
            .expect("built-in demo ABI declaration must be valid")
    }

    fn selector_case_continue(
        selector: u32,
        required_params: Vec<AbiParamRef>,
    ) -> AbiSelectorCaseDefinition {
        AbiSelectorCaseDefinition {
            selector,
            behavior: AbiBehaviorDefinition {
                required_params,
                writes: vec![AbiWriteDefinition {
                    dst: RiscvRegister(10),
                    value: AbiWriteValue::HostReturnWord,
                }],
                control: AbiControlDefinition::Continue,
            },
        }
    }
}

fn validate_identity(kind: &'static str, identity: &str) -> Result<(), RiscvAbiBuildError> {
    if identity.trim().is_empty() {
        return Err(RiscvAbiBuildError::EmptyIdentity { kind });
    }
    Ok(())
}

fn validate_register(register: RiscvRegister) -> Result<(), RiscvAbiBuildError> {
    if register.number() >= 32 {
        return Err(RiscvAbiBuildError::InvalidRegister { register: register.number() });
    }
    Ok(())
}

fn validate_entry(entry: AbiEntryContract) -> Result<(), RiscvAbiBuildError> {
    let Some(heap) = entry.guest_heap() else {
        return Ok(());
    };
    let bindings = [
        (AbiEntryValue::GuestHeapBase, heap.base()),
        (AbiEntryValue::GuestHeapLength, heap.length()),
    ];
    for (value, register) in bindings {
        validate_register(register)?;
        if register == RiscvRegister(0) {
            return Err(RiscvAbiBuildError::ZeroEntryRegister { value });
        }
        if register == RiscvRegister(2) {
            return Err(RiscvAbiBuildError::StackPointerEntryRegister { value });
        }
    }
    if heap.base() == heap.length() {
        return Err(RiscvAbiBuildError::DuplicateEntryRegister {
            first: AbiEntryValue::GuestHeapBase,
            second: AbiEntryValue::GuestHeapLength,
            register: heap.base().number(),
        });
    }
    Ok(())
}

fn validate_signal(
    signal: &AbiSignalDefinition,
    transport: AbiTransport,
) -> Result<(), RiscvAbiBuildError> {
    validate_identity("ABI signal", &signal.id.0)?;
    // Establish which architectural operands the signal can supply.
    if let AbiSignalPattern::Csrrw { csr, rd, rs1 } = signal.pattern {
        if csr > 0xfff { return Err(RiscvAbiBuildError::InvalidCsr { csr }); }
        for matcher in [rd, rs1] {
            if let RegisterMatcher::Exact(register) = matcher { validate_register(register)?; }
        }
    }
    let mut params = BTreeSet::new();
    let mut names = BTreeSet::new();
    for param in &signal.params {
        validate_identity("ABI parameter", &param.name)?;
        if !names.insert(&param.name) {
            return Err(RiscvAbiBuildError::DuplicateParamName {
                signal: signal.id.clone(), name: param.name.clone(),
            });
        }
        match param.source {
            AbiParamDefinitionSource::Register(register) => validate_register(register)?,
            AbiParamDefinitionSource::OperandRs1Value | AbiParamDefinitionSource::OperandRdRegister
                if signal.pattern == AbiSignalPattern::Ecall => {
                return Err(RiscvAbiBuildError::UnavailableOperand { signal: signal.id.clone() });
            }
            _ => {}
        }
        validate_param_ref(signal, &param.reference, &params, true)?;
        if !params.insert(param.reference.id) {
            return Err(RiscvAbiBuildError::DuplicateParam {
                signal: signal.id.clone(),
                param: param.reference.id,
            });
        }
    }

    // Admit selector/control references only after the parameter namespace is complete.
    match &signal.behavior {
        AbiBehaviorSelectionDefinition::SelectorTable {
            selector,
            cases,
            unknown_policy: _,
        } => {
            validate_param_ref(signal, selector, &params, false)?;
            let selector_param = signal
                .params
                .iter()
                .find(|param| param.reference.id == selector.id)
                .expect("validated selector parameter must exist");
            if !matches!(selector_param.source, AbiParamDefinitionSource::Register(_)) {
                return Err(RiscvAbiBuildError::NonRegisterSelector {
                    signal: signal.id.clone(),
                    param: selector.id,
                });
            }

            let mut selectors = BTreeSet::new();
            for case in cases {
                if !selectors.insert(case.selector) {
                    return Err(RiscvAbiBuildError::DuplicateSelector {
                        signal: signal.id.clone(),
                        selector: case.selector,
                    });
                }
                validate_behavior(signal, &case.behavior, &params, transport)?;
            }
        }
        AbiBehaviorSelectionDefinition::Fixed(behavior) => {
            validate_behavior(signal, behavior, &params, transport)?;
        }
    }

    Ok(())
}

fn validate_param_ref(
    signal: &AbiSignalDefinition,
    reference: &AbiParamRef,
    declared: &BTreeSet<AbiParamId>,
    declaring: bool,
) -> Result<(), RiscvAbiBuildError> {
    if reference.signal != signal.id || (!declaring && !declared.contains(&reference.id)) {
        return Err(RiscvAbiBuildError::UndeclaredParam {
            signal: signal.id.clone(),
            param: reference.id,
        });
    }
    Ok(())
}

fn validate_behavior(
    signal: &AbiSignalDefinition,
    behavior: &AbiBehaviorDefinition,
    params: &BTreeSet<AbiParamId>,
    transport: AbiTransport,
) -> Result<(), RiscvAbiBuildError> {
    let mut required = BTreeSet::new();
    for param in &behavior.required_params {
        validate_param_ref(signal, param, params, false)?;
        if !required.insert(param.id) {
            return Err(RiscvAbiBuildError::DuplicateRequiredParam {
                signal: signal.id.clone(),
                param: param.id,
            });
        }
    }

    let mut writes = BTreeSet::new();
    for write in &behavior.writes {
        validate_register(write.dst)?;
        if write.dst.number() == 0 {
            return Err(RiscvAbiBuildError::ZeroRegisterWrite {
                signal: signal.id.clone(),
            });
        }
        if !writes.insert(write.dst) {
            return Err(RiscvAbiBuildError::DuplicateWrite {
                signal: signal.id.clone(),
                register: write.dst.number(),
            });
        }
        match (transport, write.value) {
            (AbiTransport::GenericHostFfi, AbiWriteValue::HostReturnWord) => {}
        }
    }

    if let AbiControlDefinition::TerminatesComplete { output } = &behavior.control {
        match output {
            AbiOutputDefinition::Slice { ptr, len } => {
                validate_param_ref(signal, ptr, params, false)?;
                validate_param_ref(signal, len, params, false)?;
                for output in [ptr, len] {
                    if !required.contains(&output.id) {
                        return Err(RiscvAbiBuildError::OutputParamNotRequired {
                            signal: signal.id.clone(),
                            param: output.id,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}


impl AbiParamRef {
    /// Signal-local identity, without mutable access to the owning declaration.
    pub fn id(&self) -> AbiParamId { self.id }
}

// Wire construction is private: deserialization must pass the canonical builder before use.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AbiDeclaration {
    id: AbiDialectId,
    transport: AbiTransport,
    entry: AbiEntryContract,
    signals: Vec<AbiSignalDefinition>,
}
impl TryFrom<AbiDeclaration> for RiscvAbi {
    type Error = RiscvAbiBuildError;
    fn try_from(value: AbiDeclaration) -> Result<Self, Self::Error> {
        RiscvAbiBuilder { id: value.id, transport: Some(value.transport), entry: value.entry,
            signals: value.signals }.build()
    }
}
