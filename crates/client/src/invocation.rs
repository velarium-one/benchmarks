use std::{ffi::c_void, marker::PhantomData, ptr};
use config::ffi::{self, Bytes, ClientCallbacks, InvocationBindings, PreparedHandle, ResultsHandle};
use crate::{Error, Result, Session, error::bytes};

/// Invocation-bound client state and input. Callback code/state types remain an unsafe contract.
pub struct ClientBindings<'client> {
    raw: InvocationBindings,
    _client: PhantomData<&'client mut c_void>,
}
impl<'client> ClientBindings<'client> {
    /// # Safety
    /// Callbacks must correctly interpret this state, obey callback-scoped pointer/access rules,
    /// remain loaded through invoke/discard, and never unwind through C. Gmem is trusted transport,
    /// not a fully accessible slice; each requested buffer must satisfy its operation's contract.
    pub unsafe fn new<T>(callbacks: ClientCallbacks, state: &'client mut T, input: &'client [u8]) -> Self {
        let raw = InvocationBindings {
            callbacks,
            context: (state as *mut T).cast(),
            input: Bytes::borrowed(input),
        };

        Self { raw, _client: PhantomData }
    }
}

/// Single-use readiness, retaining the session and the client's borrow until invoke or discard.
pub struct PreparedInvocation<'client> {
    raw: *mut PreparedHandle,
    session: Session,
    _client: PhantomData<&'client mut c_void>,
}

impl Session {
    pub fn prepare_invocation<'client>(&self, bindings: ClientBindings<'client>) -> Result<PreparedInvocation<'client>> {
        self.check_access()?;

        let engine = self.engine();
        let mut raw = ptr::null_mut();
        let mut error = ptr::null_mut();
        let status = unsafe { (engine.api().prepare_invocation)(self.0.raw, &bindings.raw, &mut raw, &mut error) };
        engine.finish(status, error)?;

        if raw.is_null() {
            return Err(Error::contract("preparation success omitted handle"));
        }

        Ok(PreparedInvocation { raw, session: self.clone(), _client: PhantomData })
    }
}

impl PreparedInvocation<'_> {
    /// Consume readiness even if native entry is rejected. Preparation/reset is not repeated.
    pub fn invoke(mut self) -> Result<InvocationResults> {
        self.session.check_access()?;

        let engine = self.session.engine();
        let mut raw = ptr::null_mut();
        let mut error = ptr::null_mut();
        // The engine consumes the preparation slot on either success or entry failure.
        let status = unsafe { (engine.api().invoke)(&mut self.raw, &mut raw, &mut error) };
        engine.finish(status, error)?;

        if raw.is_null() {
            return Err(Error::contract("invocation success omitted results"));
        }

        Ok(InvocationResults { raw, session: self.session.clone() })
    }
}

impl Drop for PreparedInvocation<'_> {
    fn drop(&mut self) {
        if self.raw.is_null() {
            return;
        }
        self.session.check_access().expect("prepared session cannot also have a result borrow");

        let engine = self.session.engine();
        let mut error = ptr::null_mut();
        let status = unsafe { (engine.api().discard_invocation)(&mut self.raw, &mut error) };

        engine.finish(status, error).expect("safe preparation discard invariant");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome { Completed, ClientTerminated }
#[derive(Clone, Copy, Debug)]
pub struct Counter<'a> { pub name: &'a str, pub value: u64 }
pub struct ResultView<'a> {
    pub outcome: Outcome,
    pub output: Option<&'a [u8]>,
    pub counters: &'a [Counter<'a>],
}

/// Retains storage, but not its validity: the next preparation expires these results.
pub struct InvocationResults {
    raw: *mut ResultsHandle,
    session: Session,
}

struct ResultAccess<'a>(&'a Session);
impl Drop for ResultAccess<'_> {
    fn drop(&mut self) {
        self.0.0.accessing.set(false);
    }
}

impl InvocationResults {
    /// Borrow checked output and counters only for this closure; return owned data if needed.
    /// The reservation is shared across cloned sessions and released on normal return or unwind.
    pub fn with_results<T>(&self, callback: impl for<'a> FnOnce(ResultView<'a>) -> T) -> Result<T> {
        // Reserve before asking the engine, and retain through the complete caller closure.
        self.session.check_access()?;
        self.session.0.accessing.set(true);
        let _access = ResultAccess(&self.session);

        let engine = self.session.engine();
        let mut view = ffi::InvocationView::EMPTY;
        let mut error = ptr::null_mut();
        let status = unsafe { (engine.api().result_view)(self.raw, &mut view, &mut error) };
        engine.finish(status, error)?;

        // Admit wire alternatives and extents before exposing Rust references.
        let outcome = match view.outcome {
            ffi::OUTCOME_COMPLETED => Outcome::Completed,
            ffi::OUTCOME_CLIENT_TERMINATED => Outcome::ClientTerminated,
            _ => return Err(Error::contract("unknown invocation outcome")),
        };
        let output = match view.output_present {
            0 => None,
            1 => Some(unsafe { bytes(view.output)? }),
            _ => return Err(Error::contract("invalid output presence")),
        };

        // Counter descriptors borrow engine storage; their Rust projection lives through the closure.
        let counter_count = usize::try_from(view.counters_len)
            .map_err(|_| Error::contract("counter length exceeds host"))?;
        if counter_count > isize::MAX as usize / std::mem::size_of::<ffi::CounterView>() {
            return Err(Error::contract("counter extent exceeds slice limit"));
        }
        let wire_counters = if counter_count == 0 {
            &[]
        } else {
            if view.counters.is_null() || !view.counters.is_aligned() {
                return Err(Error::contract("invalid counter array"));
            }
            unsafe { std::slice::from_raw_parts(view.counters, counter_count) }
        };
        let counters = wire_counters.iter()
            .map(|entry| {
                let name_bytes = unsafe { bytes(entry.name)? };
                let name = std::str::from_utf8(name_bytes)
                    .map_err(|_| Error::contract("counter name is not UTF-8"))?;

                Ok(Counter { name, value: entry.value })
            })
            .collect::<Result<Vec<_>>>()?;

        // Each outcome admits a distinct publication shape, independent of descriptor validity.
        let publication_matches_outcome = match outcome {
            Outcome::Completed => output.is_some(),
            Outcome::ClientTerminated => output.is_none() && counters.is_empty(),
        };
        if !publication_matches_outcome {
            return Err(Error::contract("outcome has inconsistent output/counters"));
        }

        let results = ResultView { outcome, output, counters: &counters };
        Ok(callback(results))
    }
}

impl Drop for InvocationResults {
    fn drop(&mut self) {
        let engine = self.session.engine();
        let mut error = ptr::null_mut();
        // Releasing a different result does not mutate guest storage. The result currently
        // borrowed by a closure retains the session until its access guard is gone.
        let status = unsafe { (engine.api().release_results)(&mut self.raw, &mut error) };

        engine.finish(status, error).expect("safe result release invariant");
    }
}
