# Builds the nucleation Python extension from the Diplomat-generated nanobind
# sources in src/, statically linking the Rust core. Invoked via scikit-build-core
# (`pip install bindings/python`), not directly.
cmake_minimum_required(VERSION 3.15...3.27)
project(nucleation_ext LANGUAGES CXX)

if (NOT SKBUILD)
  message(WARNING "Build via 'pip install ${CMAKE_CURRENT_SOURCE_DIR}' (scikit-build-core), not raw CMake.")
endif()

find_package(Python 3.9
  REQUIRED COMPONENTS Interpreter Development.Module
  OPTIONAL_COMPONENTS Development.SABIModule)

# nanobind from the pip build env (declared in pyproject build-system.requires)
execute_process(
  COMMAND "${Python_EXECUTABLE}" -m nanobind --cmake_dir
  OUTPUT_STRIP_TRAILING_WHITESPACE OUTPUT_VARIABLE nanobind_ROOT)
find_package(nanobind CONFIG REQUIRED)

if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/rust/Cargo.toml")
  # Self-contained source distribution staged by tools/stage-python-sdist.py.
  set(REPO_ROOT ${CMAKE_CURRENT_SOURCE_DIR}/rust)
else()
  # Normal checkout/editable build.
  set(REPO_ROOT ${CMAKE_CURRENT_SOURCE_DIR}/../..)
endif()
if(DEFINED ENV{CARGO_TARGET_DIR} AND NOT "$ENV{CARGO_TARGET_DIR}" STREQUAL "")
  get_filename_component(CARGO_TARGET_DIR "$ENV{CARGO_TARGET_DIR}" ABSOLUTE
                         BASE_DIR "${REPO_ROOT}")
else()
  set(CARGO_TARGET_DIR "${REPO_ROOT}/target")
endif()
set(RUST_LIB ${CARGO_TARGET_DIR}/release/${CMAKE_STATIC_LIBRARY_PREFIX}nucleation${CMAKE_STATIC_LIBRARY_SUFFIX})

# NUCLEATION_FEATURES lets CI choose the feature set baked into the wheel.
if(NOT DEFINED ENV{NUCLEATION_FEATURES})
  set(NUCLEATION_FEATURES "bridge-full")
else()
  set(NUCLEATION_FEATURES $ENV{NUCLEATION_FEATURES})
endif()

# Always invoke cargo (it is incremental and no-ops when fresh): the previous
# OUTPUT-based rule skipped the build whenever a libnucleation.a already
# existed, silently linking stale symbols after source changes.
add_custom_target(nucleation_rust_target
  COMMAND cargo build --release --lib --features ${NUCLEATION_FEATURES}
  BYPRODUCTS ${RUST_LIB}
  WORKING_DIRECTORY ${REPO_ROOT})
add_library(nucleation_rust STATIC IMPORTED GLOBAL)
add_dependencies(nucleation_rust nucleation_rust_target)
set_target_properties(nucleation_rust PROPERTIES IMPORTED_LOCATION ${RUST_LIB})
if(WIN32)
  # propsys/runtimeobject/bcrypt/d3dcompiler/opengl32: wgpu's DX12/GL
  # backends (rendering feature, via windows_core) need them at link time.
  target_link_libraries(nucleation_rust INTERFACE kernel32.lib advapi32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib
    propsys.lib runtimeobject.lib bcrypt.lib d3dcompiler.lib opengl32.lib)
endif()
if(APPLE)
  # wgpu (rendering feature) needs the system frameworks
  # CoreGraphics: wgpu_hal color-space symbols used by Metal surfaces
  # Foundation: wgpu_hal's Metal layer observer uses NSKeyValueChange* symbols
  target_link_libraries(nucleation_rust INTERFACE "-framework Metal" "-framework QuartzCore" "-framework CoreGraphics" "-framework CoreFoundation" "-framework Foundation")
endif()

# Resolve feature aliases from Cargo and cfg gates from the Rust bridge. The
# generated entry point must omit the same registrations as the source list.
set(BINDING_BUILD_DIR "${CMAKE_CURRENT_BINARY_DIR}/selected-bindings")
file(GLOB BRIDGE_SOURCES "${REPO_ROOT}/src/bridge/*.rs")
file(GLOB_RECURSE BINDING_SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/src/*.cpp")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
  "${REPO_ROOT}/Cargo.toml" ${BRIDGE_SOURCES} ${BINDING_SOURCES}
  "${CMAKE_CURRENT_SOURCE_DIR}/configure_bindings.py")
execute_process(
  COMMAND "${Python_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/configure_bindings.py"
    --repo "${REPO_ROOT}" --output "${BINDING_BUILD_DIR}" --features "${NUCLEATION_FEATURES}"
  RESULT_VARIABLE BINDING_CONFIG_RESULT)
if(NOT BINDING_CONFIG_RESULT EQUAL 0)
  message(FATAL_ERROR "Could not select Python bindings for NUCLEATION_FEATURES=${NUCLEATION_FEATURES}")
endif()
include("${BINDING_BUILD_DIR}/bindings.cmake")
nanobind_add_module(
  nucleation
  STABLE_ABI
  NB_STATIC
  "${BINDING_BUILD_DIR}/nucleation_ext.cpp"
  ${SUBMODULE_FILES}
)
target_include_directories(nucleation PUBLIC "src/include")
target_include_directories(nucleation PRIVATE "custom")
# The Diplomat-generated dealloc shim reaches into nanobind's internal
# nb_inst struct (the reason for the ==2.12.0 pin), so the generated
# sources compile against nb_internals.h, which needs nanobind's bundled
# robin_map headers — private to the nanobind target, so add them here.
execute_process(
  COMMAND "${Python_EXECUTABLE}" -c "import nanobind, os; print(os.path.dirname(nanobind.__file__))"
  OUTPUT_STRIP_TRAILING_WHITESPACE OUTPUT_VARIABLE NB_PKG_DIR)
target_include_directories(nucleation PRIVATE "${NB_PKG_DIR}/ext/robin_map/include")
target_compile_features(nucleation PUBLIC cxx_std_20)
target_link_libraries(nucleation PRIVATE nucleation_rust)
# nanobind_add_module chooses Python::Module / Python::SABIModule. These
# imported targets own the platform-specific ABI linkage; do not read a
# Python3_* variable after find_package(Python).
# Native Termux may identify the CMake host as Linux rather than Android.
execute_process(
  COMMAND "${Python_EXECUTABLE}" -c "import sys; print(int(hasattr(sys, 'getandroidapilevel') or sys.platform == 'android'))"
  OUTPUT_VARIABLE PYTHON_ON_ANDROID OUTPUT_STRIP_TRAILING_WHITESPACE
  RESULT_VARIABLE PYTHON_PLATFORM_RESULT)
if(NOT PYTHON_PLATFORM_RESULT EQUAL 0)
  message(FATAL_ERROR "Could not identify the Python target platform")
endif()
if(ANDROID OR CMAKE_SYSTEM_NAME STREQUAL "Android" OR PYTHON_ON_ANDROID STREQUAL "1")
  # Bionic needs explicit libpython linkage. SABIModule can be a library-less
  # interface target on Unix, so discover the embedding library for this host.
  find_package(Python ${Python_VERSION} EXACT REQUIRED COMPONENTS Interpreter Development.Embed)
  target_link_libraries(nucleation PRIVATE Python::Python android log)
endif()

# Generate type information from nanobind's structured signatures. The
# extension is a top-level module in the build tree and becomes the
# `nucleation.nucleation` submodule when installed below. Keep that native
# stub beside the extension, and also compose a package-level `__init__.pyi`
# with explicit aliases. A wildcard-only re-export is understood by Mypy but
# not consistently by Pyright when the source is a native submodule.
nanobind_add_stub(
  nucleation_stub
  MODULE nucleation
  OUTPUT nucleation.pyi
  PYTHON_PATH $<TARGET_FILE_DIR:nucleation>
  DEPENDS nucleation
  MARKER_FILE py.typed
)
add_custom_target(
  nucleation_stub_normalized ALL
  COMMAND
    "${Python_EXECUTABLE}"
    "${CMAKE_CURRENT_SOURCE_DIR}/normalize_stub.py"
    "${CMAKE_CURRENT_BINARY_DIR}/nucleation.pyi"
    "${CMAKE_CURRENT_BINARY_DIR}/__init__.pyi"
    "${CMAKE_CURRENT_SOURCE_DIR}/nucleation/__init__.pyi"
  DEPENDS nucleation_stub
  VERBATIM
)

# The extension lands INSIDE the veneer package (nucleation/) so that
# `import nucleation` resolves to the hand-written Python layer, which
# re-exports the compiled core as its `nucleation.nucleation` submodule
# (the PyInit_nucleation symbol matches the last dotted component).
install(TARGETS nucleation LIBRARY DESTINATION nucleation)
install(
  FILES
    "${CMAKE_CURRENT_BINARY_DIR}/nucleation.pyi"
    "${CMAKE_CURRENT_BINARY_DIR}/__init__.pyi"
    "${CMAKE_CURRENT_BINARY_DIR}/py.typed"
  DESTINATION nucleation
)
