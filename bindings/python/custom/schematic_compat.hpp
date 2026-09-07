#pragma once

#include "Schematic.hpp"

#include <string>
#include <utility>

namespace nucleation::python_compat {

inline std::string filesystem_path(nb::handle path) {
    nb::object normalized = nb::module_::import_("os").attr("fspath")(path);
    if (!nb::isinstance<nb::str>(normalized)) {
        throw nb::type_error("path must be str or os.PathLike[str]");
    }
    return nb::cast<std::string>(normalized);
}

inline diplomat::result<std::unique_ptr<Schematic>, NucleationError>
schematic_open(nb::handle path) {
    return Schematic::open(filesystem_path(path));
}

inline void unwrap_void_result(
    diplomat::result<std::monostate, NucleationError> result
) {
    if (result.is_ok()) {
        return;
    }

    NucleationError error = std::move(result).err().value();
    nb::object python_error = nb::cast(error);
    set_result_error(python_error.ptr());
    throw nb::python_error();
}

inline void schematic_save(
    const Schematic& schematic,
    nb::handle path,
    nb::object format
) {
    std::string normalized_path = filesystem_path(path);
    if (format.is_none()) {
        unwrap_void_result(schematic.save(normalized_path));
        return;
    }

    std::string normalized_format = nb::cast<std::string>(format);
    unwrap_void_result(schematic.save_to_file_with_format(
        normalized_path,
        normalized_format,
        ""
    ));
}

} // namespace nucleation::python_compat
