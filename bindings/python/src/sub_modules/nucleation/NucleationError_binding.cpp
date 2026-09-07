#include "diplomat_nanobind_common.hpp"


#include "NucleationError.hpp"

namespace nucleation {
void add_NucleationError_binding(nb::module_ mod) {
    nb::class_<nucleation::NucleationError> e_class(mod, "NucleationErrorCode");

        nb::enum_<nucleation::NucleationError::Value> enumerator(e_class, "NucleationErrorCode");
        enumerator
            .value("NullArgument", nucleation::NucleationError::NullArgument)
            .value("InvalidArgument", nucleation::NucleationError::InvalidArgument)
            .value("Parse", nucleation::NucleationError::Parse)
            .value("Serialize", nucleation::NucleationError::Serialize)
            .value("Io", nucleation::NucleationError::Io)
            .value("Lock", nucleation::NucleationError::Lock)
            .value("Store", nucleation::NucleationError::Store)
            .value("Mesh", nucleation::NucleationError::Mesh)
            .value("Render", nucleation::NucleationError::Render)
            .value("Simulation", nucleation::NucleationError::Simulation)
            .value("AlreadyConsumed", nucleation::NucleationError::AlreadyConsumed)
            .value("NotFound", nucleation::NucleationError::NotFound)
            .value("Generation", nucleation::NucleationError::Generation)
            .export_values();

        e_class
            .def(nb::init_implicit<nucleation::NucleationError::Value>())
            .def(nb::self == nucleation::NucleationError::Value())
            .def("__repr__", [](const nucleation::NucleationError& self){
                return nb::str(nb::cast(nucleation::NucleationError::Value(self)));
            });
    nucleation::python_compat::register_error_type(mod, e_class);

}

}
