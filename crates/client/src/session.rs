use std::{cell::Cell, ptr, rc::Rc};
use config::ffi::SessionHandle;
use crate::{Engine, Error, Program, Result};

/// [nb:core] Reusable execution storage shared by serial invocations, not by concurrent entry.
/// A temporary result reservation prevents safe reentrant invalidation across every handle.
#[derive(Clone)]
pub struct Session(pub(crate) Rc<SessionInner>);
pub(crate) struct SessionInner {
    pub raw: *mut SessionHandle,
    pub program: Program,
    pub accessing: Cell<bool>,
}

impl Program {
    pub fn create_session(&self, gmem_capacity: u64) -> Result<Session> {
        let mut raw = ptr::null_mut();
        let mut error = ptr::null_mut();
        let engine = &self.0.engine;
        let status = unsafe { (engine.api().create_session)(self.0.raw, gmem_capacity, &mut raw, &mut error) };
        engine.finish(status, error)?;
        if raw.is_null() { return Err(Error::contract("session success omitted handle")); }
        Ok(Session(Rc::new(SessionInner { raw, program: self.clone(), accessing: Cell::new(false) })))
    }
}

impl Session {
    pub(crate) fn engine(&self) -> &Engine { &self.0.program.0.engine }
    pub(crate) fn check_access(&self) -> Result<()> {
        if self.0.accessing.get() { Err(Error::contract("session results are currently borrowed")) } else { Ok(()) }
    }
}

impl Drop for SessionInner {
    fn drop(&mut self) {
        assert!(!self.accessing.get(), "result access retains its session");
        let mut error = ptr::null_mut();
        let engine = &self.program.0.engine;
        let status = unsafe { (engine.api().release_session)(&mut self.raw, &mut error) };
        engine.finish(status, error).expect("safe Session release invariant");
    }
}
