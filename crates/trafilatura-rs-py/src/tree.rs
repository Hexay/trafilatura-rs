//! lxml trees as input: trafilatura 2.3.1's `bare_extraction` works on `copy(tree)`, which `dom::copy_foreign`
//! makes from the `xmlNode` behind the `lxml.etree._Element`.
use crate::{to_py_err, Unsupported};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyString, PyType};
use std::ffi::c_void;
use trafilatura_rs::dom::{copy_foreign, Ctx, Node};

/// lxml's public `struct LxmlElement` (lxml/etree.h).
#[repr(C)]
struct LxmlElement {
    ob_base: pyo3::ffi::PyObject,
    doc: *mut pyo3::ffi::PyObject,
    c_node: *const c_void,
    tag: *mut pyo3::ffi::PyObject,
}

/// An extraction's input: the HTML string, or the copied tree.
pub(crate) enum Input<'a> {
    Html(&'a str),
    Tree(Node),
}

impl<'a> Input<'a> {
    /// `obj` as a str, or as `copy(obj)` in `ctx` for an `lxml.etree._Element`.
    pub(crate) fn new(ctx: &Ctx, obj: &'a Bound<'_, PyAny>) -> PyResult<Input<'a>> {
        if let Ok(s) = obj.cast::<PyString>() {
            return Ok(Input::Html(s.to_str()?));
        }
        static ELEMENT: PyOnceLock<Py<PyType>> = PyOnceLock::new();
        let element = ELEMENT
            .import(obj.py(), "lxml.etree", "_Element")
            .map_err(|_| Unsupported::new_err("input is neither str nor an lxml element"))?;
        if !obj.is_instance(element)? {
            return Err(Unsupported::new_err("input is neither str nor an lxml element"));
        }
        // the GIL is held: nothing mutates the tree while it is copied
        let node = unsafe { (*(obj.as_ptr() as *const LxmlElement)).c_node };
        unsafe { copy_foreign(ctx, node) }.map(Input::Tree).map_err(to_py_err)
    }
}

/// Moves `Ctx`-bound values into `Python::detach`, which runs the closure on this same thread.
pub(crate) struct SameThread<T>(pub T);

unsafe impl<T> Send for SameThread<T> {}

impl<T> SameThread<T> {
    /// By value, so a closure captures the whole wrapper rather than its (non-Send) field.
    pub(crate) fn get(self) -> T {
        self.0
    }
}
