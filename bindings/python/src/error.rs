use pyo3::exceptions;
use pyo3::prelude::*;
use pyo3::type_object::PyTypeInfo;
use std::ffi::CString;
use std::fmt::{Display, Formatter, Result as FmtResult};
use tokenizers::tokenizer::Result;

#[derive(Debug)]
pub struct PyError(pub String);
impl PyError {
    #[allow(dead_code)]
    pub fn from(s: &str) -> Self {
        PyError(String::from(s))
    }
    pub fn into_pyerr<T: PyTypeInfo>(self) -> PyErr {
        PyErr::new::<T, _>(format!("{self}"))
    }
}
impl Display for PyError {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        write!(fmt, "{}", self.0)
    }
}
impl std::error::Error for PyError {}

pub struct ToPyResult<T>(pub Result<T>);
impl<T> From<ToPyResult<T>> for PyResult<T> {
    fn from(v: ToPyResult<T>) -> Self {
        v.0.map_err(|e| exceptions::PyException::new_err(format!("{e}")))
    }
}
impl<T> ToPyResult<T> {
    pub fn into_py(self) -> PyResult<T> {
        self.into()
    }
}

pub(crate) fn deprecation_warning(py: Python<'_>, version: &str, message: &str) -> PyResult<()> {
    let deprecation_warning = py.import("builtins")?.getattr("DeprecationWarning")?;
    let full_message = format!("Deprecated in {version}: {message}");
    pyo3::PyErr::warn(py, &deprecation_warning, &CString::new(full_message)?, 0)
}
