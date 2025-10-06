use std::ffi::CString;
use std::fs::read_to_string;
use std::path::Path;

use pyo3::prelude::{Py, PyAny, PyModule, PyTracebackMethods};
use pyo3_ffi::c_str;

pub struct Script {
    name: String,
    module: Option<Py<PyModule>>,
}

impl Script {
    pub fn empty() -> Self {
        Self {
            name: "".to_owned(),
            module: None,
        }
    }

    pub fn initialize() {
        pyo3::Python::initialize();
    }

    pub fn get_name(&self) -> &str {
        self.name.as_str()
    }

    pub fn new(file_name: &str) -> Self {
        let file_name_c_str = CString::new(file_name);
        Self {
            name: file_name.to_owned(),
            module: read_to_string(Path::new(file_name)).ok().and_then(|script| {
                pyo3::Python::attach(|py| -> Option<Py<PyModule>> {
                    PyModule::from_code(
                        py,
                        CString::new(script).ok()?.as_c_str(),
                        file_name_c_str.ok()?.as_c_str(),
                        c_str!(""), // todo: add later
                    )
                    .map(|m| m.unbind())
                    .ok()
                })
            }),
        }
    }

    pub fn evaluate(&self, b: &str, c: &str, return_traceback: bool) -> Option<String> {
        self.module.as_ref().and_then(|module| {
            pyo3::Python::attach(|py| -> Option<String> {
                let app: Py<PyAny> = module.getattr(py, b).ok()?; // Great API design in pyo3!
                match app.call1(py, (c,)) {
                    Ok(v) => Some(v.to_string()),
                    Err(e) => {
                        if return_traceback {
                            match e.traceback(py).and_then(|f| f.format().ok()) {
                                Some(t) => Some(format!("{t}{e}")),
                                None => Some(e.to_string()),
                            }
                        } else {
                            e.print(py);
                            None
                        }
                    }
                }
            })
        })
    }
}
