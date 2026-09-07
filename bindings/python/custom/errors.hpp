#pragma once

// Result casters are noexcept: use the C API and propagate allocation errors.
// The enum's Python type owns the exception reference (no process-global PyObject
// pointers, which could outlive an interpreter).
namespace nucleation::python_compat {
inline void set_result_error(PyObject* code) noexcept {
    PyObject* type = PyObject_GetAttrString((PyObject*) Py_TYPE(code), "__exception_type__");
    if (!type) {
        if (PyErr_ExceptionMatches(PyExc_AttributeError)) {
            PyErr_Clear();
            PyErr_SetObject(PyExc_Exception, code);
        }
        return;
    }
    PyObject* message = PyObject_Str(code);
    PyObject* error = message ? PyObject_CallFunctionObjArgs(type, message, nullptr) : nullptr;
    if (error && PyObject_SetAttrString(error, "code", code) == 0) {
        PyErr_SetObject(type, error);
    }
    Py_XDECREF(error);
    Py_XDECREF(message);
    Py_DECREF(type);
}

inline void register_error_type(nb::module_ mod, nb::handle code_type) {
    nb::object exception = nb::steal<nb::object>(
        PyErr_NewException("nucleation.NucleationError", PyExc_Exception, nullptr));
    if (!exception.is_valid()) throw nb::python_error();
    exception.attr("__doc__") = "Native Nucleation failure. The code attribute is a NucleationErrorCode.";
    code_type.attr("__exception_type__") = exception;
    mod.attr("NucleationError") = exception;
    // Retain the old spelling of constants for existing callers.
    for (const char* name : {"NullArgument", "InvalidArgument", "Parse", "Serialize",
                            "Io", "Lock", "Store", "Mesh", "Render", "Simulation",
                            "AlreadyConsumed", "NotFound", "Generation"}) {
        exception.attr(name) = code_type.attr(name);
    }
}
} // namespace nucleation::python_compat
