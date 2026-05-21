use std::ffi::CString;
use std::fs::read_to_string;
use std::path::Path;

use pyo3::prelude::{FromPyObject, Py, PyAny, PyModule, PyTracebackMethods, Python};
use pyo3_ffi::c_str;

pub struct Script {
    name: String,
    module: Option<Py<PyModule>>,
}

impl Script {
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            module: None,
        }
    }

    pub fn initialize() {
        Python::initialize();
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn new(file_name: &str) -> Self {
        let file_name_c_str = CString::new(file_name);
        Self {
            name: file_name.to_owned(),
            module: read_to_string(Path::new(file_name)).ok().and_then(|script| {
                Python::attach(|py| -> Option<Py<PyModule>> {
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

    pub fn getattr<T: for<'py> FromPyObject<'py, 'py>>(&self, v: &str) -> Option<T> {
        self.module.as_ref().and_then(|module| {
            Python::attach(|py| -> Option<T> {
                let obj: Py<PyAny> = module.getattr(py, v).ok()?;
                obj.extract(py).ok()
            })
        })
    }

    pub fn call_hook(
        &self,
        func_name: &str,
        args: (&str, &str, Option<&str>, Option<i64>),
    ) -> Option<(String, String, Option<String>, Option<i64>)> {
        self.module.as_ref().and_then(|module| {
            Python::attach(|py| -> Option<(String, String, Option<String>, Option<i64>)> {
                let func: Py<PyAny> = match module.getattr(py, func_name) {
                    Ok(f) => f,
                    Err(e) => {
                        println!("[Redis Hook] Failed to get function '{}': {}", func_name, e);
                        return None;
                    }
                };
                let (key, value, ttl_type, ttl_value) = args;
                let result = match func.call1(py, (key, value, ttl_type, ttl_value)) {
                    Ok(r) => r,
                    Err(e) => {
                        println!("[Redis Hook] Failed to call function '{}': {}", func_name, e);
                        return None;
                    }
                };
                let tuple: (&str, &str, Option<&str>, Option<i64>) = match result.extract(py) {
                    Ok(t) => t,
                    Err(e) => {
                        println!(
                            "[Redis Hook] Failed to extract return value from '{}': {}",
                            func_name, e
                        );
                        return None;
                    }
                };
                Some((
                    tuple.0.to_string(),
                    tuple.1.to_string(),
                    tuple.2.map(|s| s.to_string()),
                    tuple.3,
                ))
            })
        })
    }

    pub fn evaluate(&self, a: &str, b: &str, c: &str, return_traceback: bool) -> Option<String> {
        self.module.as_ref().and_then(|module| {
            Python::attach(|py| -> Option<String> {
                let app: Py<PyAny> = module.getattr(py, format!("{}_{}", a, b)).ok()?; // Great API design in pyo3!
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
