#![no_std]

use wasmparser::{Encoding, Operator, Parser, Payload, TypeRef};

#[cfg(target_arch = "riscv32")]
mod guest;
#[cfg(target_arch = "riscv32")]
pub use guest::{WasmFixtureConfig, run};

const FNV1A_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV1A_PRIME: u32 = 0x0100_0193;

/// Compact structural evidence from one complete core WebAssembly parse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct WasmFixtureResult {
    pub input_length: u32,
    pub section_count: u32,
    pub custom_section_count: u32,
    pub type_count: u32,
    pub imported_function_count: u32,
    pub defined_function_count: u32,
    pub table_count: u32,
    pub memory_count: u32,
    pub global_count: u32,
    pub export_count: u32,
    pub element_count: u32,
    pub data_segment_count: u32,
    pub code_body_count: u32,
    pub operator_count: u32,
    pub control_operator_count: u32,
    pub call_operator_count: u32,
    pub parametric_operator_count: u32,
    pub variable_operator_count: u32,
    pub memory_operator_count: u32,
    pub numeric_operator_count: u32,
    pub reference_table_operator_count: u32,
    pub facts_fnv1a32: u32,
}

impl WasmFixtureResult {
    pub const WORDS: usize = 22;
    pub const BYTE_LEN: usize = Self::WORDS * size_of::<u32>();

    /// Encodes the fixture result in the little-endian word format returned to the harness.
    pub fn to_le_bytes(self) -> [u8; Self::BYTE_LEN] {
        let words = self.words();
        let mut output = [0_u8; Self::BYTE_LEN];
        let mut index = 0;
        while index < words.len() {
            let offset = index * size_of::<u32>();
            output[offset..offset + size_of::<u32>()]
                .copy_from_slice(&words[index].to_le_bytes());
            index += 1;
        }
        output
    }

    /// Establishes the digest after independently supplied structural facts are complete.
    pub fn finalized(mut self) -> Self {
        self.establish_digest();
        self
    }

    fn words(self) -> [u32; Self::WORDS] {
        [
            self.input_length,
            self.section_count,
            self.custom_section_count,
            self.type_count,
            self.imported_function_count,
            self.defined_function_count,
            self.table_count,
            self.memory_count,
            self.global_count,
            self.export_count,
            self.element_count,
            self.data_segment_count,
            self.code_body_count,
            self.operator_count,
            self.control_operator_count,
            self.call_operator_count,
            self.parametric_operator_count,
            self.variable_operator_count,
            self.memory_operator_count,
            self.numeric_operator_count,
            self.reference_table_operator_count,
            self.facts_fnv1a32,
        ]
    }

    fn establish_digest(&mut self) {
        let mut digest = FNV1A_OFFSET_BASIS;
        for word in self.words()[..Self::WORDS - 1].iter().copied() {
            for byte in word.to_le_bytes() {
                digest = (digest ^ byte as u32).wrapping_mul(FNV1A_PRIME);
            }
        }
        self.facts_fnv1a32 = digest;
    }
}

/// Failure classes for the deliberately narrow core-module parser fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmFixtureError {
    InputTooLarge,
    Parse,
    UnsupportedEncoding,
    UnsupportedSection,
    UnsupportedOperator,
    CounterOverflow,
    FunctionCountMismatch,
    OperatorCountMismatch,
    DataCountMismatch,
}

/// Parses one complete core WebAssembly module and returns its structural result.
pub fn parse_module(bytes: &[u8]) -> Result<WasmFixtureResult, WasmFixtureError> {
    let mut result = WasmFixtureResult {
        input_length: bytes
            .len()
            .try_into()
            .map_err(|_| WasmFixtureError::InputTooLarge)?,
        ..WasmFixtureResult::default()
    };
    let mut declared_code_bodies = None;
    let mut declared_data_segments = None;

    // Traverse every section entry and every function-body operator in source order.
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|_| WasmFixtureError::Parse)? {
            Payload::Version { encoding, .. } => {
                if encoding != Encoding::Module {
                    return Err(WasmFixtureError::UnsupportedEncoding);
                }
            }
            Payload::TypeSection(reader) => {
                increment(&mut result.section_count)?;
                for item in reader {
                    item.map_err(|_| WasmFixtureError::Parse)?;
                    increment(&mut result.type_count)?;
                }
            }
            Payload::ImportSection(reader) => {
                increment(&mut result.section_count)?;
                for item in reader.into_imports() {
                    let import = item.map_err(|_| WasmFixtureError::Parse)?;
                    if matches!(import.ty, TypeRef::Func(_)) {
                        increment(&mut result.imported_function_count)?;
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                increment(&mut result.section_count)?;
                for item in reader {
                    item.map_err(|_| WasmFixtureError::Parse)?;
                    increment(&mut result.defined_function_count)?;
                }
            }
            Payload::TableSection(reader) => {
                increment(&mut result.section_count)?;
                result.table_count = consume_count(reader, result.table_count)?;
            }
            Payload::MemorySection(reader) => {
                increment(&mut result.section_count)?;
                result.memory_count = consume_count(reader, result.memory_count)?;
            }
            Payload::GlobalSection(reader) => {
                increment(&mut result.section_count)?;
                result.global_count = consume_count(reader, result.global_count)?;
            }
            Payload::ExportSection(reader) => {
                increment(&mut result.section_count)?;
                result.export_count = consume_count(reader, result.export_count)?;
            }
            Payload::StartSection { .. } => increment(&mut result.section_count)?,
            Payload::ElementSection(reader) => {
                increment(&mut result.section_count)?;
                result.element_count = consume_count(reader, result.element_count)?;
            }
            Payload::DataCountSection { count, .. } => {
                increment(&mut result.section_count)?;
                declared_data_segments = Some(count);
            }
            Payload::DataSection(reader) => {
                increment(&mut result.section_count)?;
                result.data_segment_count = consume_count(reader, result.data_segment_count)?;
            }
            Payload::CodeSectionStart { count, .. } => {
                increment(&mut result.section_count)?;
                declared_code_bodies = Some(count);
            }
            Payload::CodeSectionEntry(body) => {
                let locals = body
                    .get_locals_reader()
                    .map_err(|_| WasmFixtureError::Parse)?;
                for local in locals {
                    local.map_err(|_| WasmFixtureError::Parse)?;
                }

                let mut operators = body
                    .get_operators_reader()
                    .map_err(|_| WasmFixtureError::Parse)?;
                while !operators.eof() {
                    let operator = operators.read().map_err(|_| WasmFixtureError::Parse)?;
                    record_operator(&mut result, &operator)?;
                }
                increment(&mut result.code_body_count)?;
            }
            Payload::CustomSection(_) => {
                increment(&mut result.section_count)?;
                increment(&mut result.custom_section_count)?;
            }
            Payload::UnknownSection { .. } | Payload::TagSection(_) => {
                return Err(WasmFixtureError::UnsupportedSection);
            }
            Payload::End(_) => {}
            _ => return Err(WasmFixtureError::UnsupportedSection),
        }
    }

    establish_checked_result(result, declared_code_bodies, declared_data_segments)
}

fn establish_checked_result(
    result: WasmFixtureResult,
    declared_code_bodies: Option<u32>,
    declared_data_segments: Option<u32>,
) -> Result<WasmFixtureResult, WasmFixtureError> {
    // Establish cross-section and classification conservation before publishing the result.
    let declared_code_bodies = declared_code_bodies.unwrap_or(0);
    if declared_code_bodies != result.code_body_count
        || result.defined_function_count != result.code_body_count
    {
        return Err(WasmFixtureError::FunctionCountMismatch);
    }
    if declared_data_segments.is_some_and(|count| count != result.data_segment_count) {
        return Err(WasmFixtureError::DataCountMismatch);
    }
    let classified_operators = result
        .control_operator_count
        .checked_add(result.call_operator_count)
        .and_then(|count| count.checked_add(result.parametric_operator_count))
        .and_then(|count| count.checked_add(result.variable_operator_count))
        .and_then(|count| count.checked_add(result.memory_operator_count))
        .and_then(|count| count.checked_add(result.numeric_operator_count))
        .and_then(|count| count.checked_add(result.reference_table_operator_count))
        .ok_or(WasmFixtureError::CounterOverflow)?;
    if classified_operators != result.operator_count {
        return Err(WasmFixtureError::OperatorCountMismatch);
    }
    Ok(result.finalized())
}

fn consume_count<T>(
    reader: impl IntoIterator<Item = wasmparser::Result<T>>,
    mut count: u32,
) -> Result<u32, WasmFixtureError> {
    for item in reader {
        item.map_err(|_| WasmFixtureError::Parse)?;
        increment(&mut count)?;
    }
    Ok(count)
}

fn increment(value: &mut u32) -> Result<(), WasmFixtureError> {
    *value = value
        .checked_add(1)
        .ok_or(WasmFixtureError::CounterOverflow)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperatorClass {
    Control,
    Call,
    Parametric,
    Variable,
    Memory,
    Numeric,
    ReferenceTable,
}

fn record_operator(
    result: &mut WasmFixtureResult,
    operator: &Operator<'_>,
) -> Result<(), WasmFixtureError> {
    if !is_admitted_operator(operator) {
        return Err(WasmFixtureError::UnsupportedOperator);
    }

    increment(&mut result.operator_count)?;
    let count = match operator_class(operator) {
        OperatorClass::Control => &mut result.control_operator_count,
        OperatorClass::Call => &mut result.call_operator_count,
        OperatorClass::Parametric => &mut result.parametric_operator_count,
        OperatorClass::Variable => &mut result.variable_operator_count,
        OperatorClass::Memory => &mut result.memory_operator_count,
        OperatorClass::Numeric => &mut result.numeric_operator_count,
        OperatorClass::ReferenceTable => &mut result.reference_table_operator_count,
    };
    increment(count)
}

fn operator_class(operator: &Operator<'_>) -> OperatorClass {
    use Operator::*;

    match operator {
        Unreachable | Nop | Block { .. } | Loop { .. } | If { .. } | Else | End | Br { .. }
        | BrIf { .. } | BrTable { .. } | Return => OperatorClass::Control,
        Call { .. } | CallIndirect { .. } => OperatorClass::Call,
        Drop | Select | TypedSelect { .. } | TypedSelectMulti { .. } => OperatorClass::Parametric,
        LocalGet { .. } | LocalSet { .. } | LocalTee { .. } | GlobalGet { .. }
        | GlobalSet { .. } => OperatorClass::Variable,
        I32Load { .. } | I64Load { .. } | F32Load { .. } | F64Load { .. }
        | I32Load8S { .. } | I32Load8U { .. } | I32Load16S { .. } | I32Load16U { .. }
        | I64Load8S { .. } | I64Load8U { .. } | I64Load16S { .. } | I64Load16U { .. }
        | I64Load32S { .. } | I64Load32U { .. } | I32Store { .. } | I64Store { .. }
        | F32Store { .. } | F64Store { .. } | I32Store8 { .. } | I32Store16 { .. }
        | I64Store8 { .. } | I64Store16 { .. } | I64Store32 { .. } | MemorySize { .. }
        | MemoryGrow { .. } | MemoryInit { .. } | DataDrop { .. } | MemoryCopy { .. }
        | MemoryFill { .. } => OperatorClass::Memory,
        TableInit { .. } | ElemDrop { .. } | TableCopy { .. } | RefNull { .. } | RefIsNull
        | RefFunc { .. } | TableFill { .. } | TableGet { .. } | TableSet { .. }
        | TableGrow { .. } | TableSize { .. } => OperatorClass::ReferenceTable,
        _ => OperatorClass::Numeric,
    }
}

macro_rules! admitted_proposal {
    (@mvp) => { true };
    (@bulk_memory) => { true };
    (@reference_types) => { true };
    (@saturating_float_to_int) => { true };
    (@sign_extension) => { true };
    (@$proposal:ident) => { false };
}

fn is_admitted_operator(operator: &Operator<'_>) -> bool {
    macro_rules! classify_proposal {
        ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {
            match operator {
                $(Operator::$op $( { $($arg: _),* } )? => admitted_proposal!(@$proposal),)*
                _ => false,
            }
        };
    }

    wasmparser::for_each_operator!(classify_proposal)
}
