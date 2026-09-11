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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AbiParamId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiTransport {
    GenericHostFfi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiWriteValue {
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

/// Paired architectural-register carriers for the layout-owned guest heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiGuestHeapRegisters {
    base: RiscvRegister,
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

/// Program-entry values declared by one RISC-V ABI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiEntryContract {
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

/// [nb:core] Validated declarative ABI supplied to the RISC-V frontend.
/// The declaration is program-independent. CFG construction binds it to concrete source
/// instructions and publishes its entry contract before register analysis consumes either surface.
///
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AbiDeclaration")]
pub struct RiscvAbi {
    id: AbiDialectId,
    transport: AbiTransport,
    entry: AbiEntryContract,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiSignalDefinition {
    pub id: AbiSignalId,
    pub pattern: AbiSignalPattern,
    pub params: Vec<AbiParamDefinition>,
    pub behavior: AbiBehaviorSelectionDefinition,
}

#[derive(Debug, Clone)]
/// Signal-scoped builder that mints opaque parameter references for behavior declarations.
pub struct AbiSignalBuilder {
    id: AbiSignalId,
    pattern: AbiSignalPattern,
    params: Vec<AbiParamDefinition>,
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

/// Opaque signal-local reference to one ABI parameter declaration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiParamRef {
    signal: AbiSignalId,
    id: AbiParamId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiParamDefinition {
    pub reference: AbiParamRef,
    pub name: String,
    pub source: AbiParamDefinitionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiSignalPattern {
    Ecall,
    Csrrw {
        csr: u16,
        rd: RegisterMatcher,
        rs1: RegisterMatcher,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RegisterMatcher {
    Any,
    AnyNonZero,
    Exact(RiscvRegister),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiParamDefinitionSource {
    Register(RiscvRegister),
    OperandRs1Value,
    OperandRdRegister,
    Immediate(u32),
    RawInstructionWord,
    SourcePc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiBehaviorSelectionDefinition {
    SelectorTable {
        selector: AbiParamRef,
        cases: Vec<AbiSelectorCaseDefinition>,
        unknown_policy: AbiUnknownSelectorPolicy,
    },
    Fixed(AbiBehaviorDefinition),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiSelectorCaseDefinition {
    pub selector: u32,
    pub behavior: AbiBehaviorDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiUnknownSelectorPolicy {
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiBehaviorDefinition {
    pub required_params: Vec<AbiParamRef>,
    pub writes: Vec<AbiWriteDefinition>,
    pub control: AbiControlDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiControlDefinition {
    Continue,
    TerminatesTrap,
    TerminatesComplete { output: AbiOutputDefinition },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiWriteDefinition {
    pub dst: RiscvRegister,
    pub value: AbiWriteValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbiOutputDefinition {
    Slice { ptr: AbiParamRef, len: AbiParamRef },
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
    pub fn demo_dialect() -> RiscvAbi { transport_dialect("demo_dialect", "demo.ecall") }

    /// Same transport contract with the private fixture's stable evidence identities.
    pub fn test_dialect() -> RiscvAbi { transport_dialect("test_dialect", "test.ecall") }

    fn transport_dialect(identity: &str, signal: &str) -> RiscvAbi {
        let mut ecall = AbiSignalBuilder::new(signal, AbiSignalPattern::Ecall);
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

        RiscvAbi::builder(identity)
            .transport(AbiTransport::GenericHostFfi)
            .guest_heap_registers(
                RiscvRegister(10),
                RiscvRegister(11),
            )
            .signal(signal)
            .build()
            .expect("built-in test ABI declaration must be valid")
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
