#!/usr/bin/env python3
"""Normalize and package nanobind-generated type stubs.

Diplomat represents an FFI enum as a small wrapper class containing a nested
nanobind enum with the same name. Runtime constants exported onto the wrapper
are implicitly converted wherever the wrapper is accepted. Stubgen cannot
express that implicit C++ conversion and the repeated class name also shadows
the wrapper, so model the exported constants directly as wrapper instances.

The native extension lives at ``nucleation.nucleation``, while users import
its symbols from the ``nucleation`` package. Type checkers do not consistently
follow wildcard re-exports from a native submodule. For wheels, compose a
package-level ``__init__.pyi`` containing explicit aliases to every generated
definition, followed by the hand-written veneer overlay.
"""

from __future__ import annotations

import ast
import re
import sys
from pathlib import Path


TOP_LEVEL_CLASS = re.compile(r"^class ([A-Za-z_][A-Za-z0-9_]*):$")
NESTED_ENUM = re.compile(r"^    class ([A-Za-z_][A-Za-z0-9_]*)\(enum\.Enum\):$")


def normalize(source: str) -> tuple[str, int, int, int]:
    # Stubgen occasionally emits an unsubscripted Callable for native callback
    # parameters.  Bare generics fail strict Mypy even though the generated
    # signature cannot express the callback's ABI more precisely.
    source = re.sub(r"(?<=: )Callable(?=[,)])", "Callable[..., object]", source)
    # The native exception is a Python class, while its constants originate
    # in Diplomat's nested enum. Stubgen cannot discover instance-only `code`.
    error = re.search(r"^class NucleationError\(Exception\):\n.*?(?=^class |\Z)", source, re.M | re.S)
    if error:
        constants = re.findall(r"^    (\w+): NucleationErrorCode", error[0], re.M)
        replacement = "class NucleationError(Exception):\n    code: NucleationErrorCode\n"
        replacement += "".join(f"    {name}: NucleationErrorCode\n" for name in constants)
        source = source[:error.start()] + replacement + "\n" + source[error.end():]
    source = re.sub(r"^    __exception_type__:.*\n", "", source, flags=re.M)
    lines = source.splitlines()
    output: list[str] = []
    outer: str | None = None
    nested_count = 0
    constant_count = 0
    existing_constant_count = 0
    index = 0

    while index < len(lines):
        line = lines[index]
        top_level = TOP_LEVEL_CLASS.match(line)
        if top_level:
            outer = top_level.group(1)
        elif line and not line[0].isspace():
            outer = None

        nested = NESTED_ENUM.match(line)
        if outer is not None and nested and nested.group(1) == outer:
            nested_count += 1
            index += 1
            while index < len(lines):
                candidate = lines[index]
                if not candidate or candidate.startswith("        "):
                    index += 1
                    continue
                break
            continue

        if outer is not None:
            # nanobind 2.12 may qualify annotations and enum constants through
            # both wrapper layers (`Outer.Outer`). The nested enum is removed
            # above, so collapse those references to the public wrapper before
            # matching constants and methods. Older stubgen output already uses
            # the single-qualified spelling, making this idempotent.
            line = line.replace(f"{outer}.{outer}", outer)
            if re.fullmatch(
                rf"    [A-Za-z_][A-Za-z0-9_]*: {re.escape(outer)}",
                line,
            ):
                existing_constant_count += 1

            constant = re.fullmatch(
                rf"    ([A-Za-z_][A-Za-z0-9_]*): {re.escape(outer)} = "
                rf"{re.escape(outer)}\.\1",
                line,
            )
            if constant:
                name = constant.group(1)
                output.append(f"    {name}: {outer}")
                constant_count += 1
                index += 1
                continue

            if line == f"    def __eq__(self, arg: {outer}, /) -> bool: ...":
                output.append("    def __eq__(self, arg: object, /) -> bool: ...")
                index += 1
                continue

        output.append(line)
        index += 1

    return (
        "\n".join(output) + "\n",
        nested_count,
        constant_count,
        existing_constant_count,
    )


def compose_public_stub(core_stub: str, overlay: str) -> str:
    """Return explicit native re-exports followed by the veneer imports.

    ``from .nucleation import *`` is deliberately insufficient here: Pyright
    does not treat wildcard imports from a native submodule as package-level
    exports. Explicit redundant aliases are the PEP 484 spelling for a
    re-export and retain the native classes' identities.
    """
    core_tree = ast.parse(core_stub)
    core_names = {
        node.name
        for node in core_tree.body
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef))
        and not node.name.startswith("_")
    }
    for node in core_tree.body:
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            if not node.target.id.startswith("_"):
                core_names.add(node.target.id)
        elif isinstance(node, ast.Assign):
            core_names.update(
                target.id
                for target in node.targets
                if isinstance(target, ast.Name) and not target.id.startswith("_")
            )

    overlay_tree = ast.parse(overlay)
    overlay_imports: list[ast.Import | ast.ImportFrom] = []
    overlay_names: set[str] = set()
    for node in overlay_tree.body:
        if not isinstance(node, (ast.Import, ast.ImportFrom)):
            continue
        if isinstance(node, ast.ImportFrom) and any(
            alias.name == "*" for alias in node.names
        ):
            continue
        if isinstance(node, ast.ImportFrom) and node.module == "design" and "Design" not in core_names:
            continue
        overlay_imports.append(node)
        for alias in node.names:
            public_name = alias.asname or alias.name
            if public_name != "core" and not public_name.startswith("_"):
                overlay_names.add(public_name)

    native_exports = sorted(core_names - overlay_names)
    aliases = "\n".join(f"    {name} as {name}," for name in native_exports)
    rendered_overlay = "\n".join(ast.unparse(node) for node in overlay_imports)
    return (
        "# Generated at wheel build time; do not edit this installed file.\n"
        "# Explicit aliases make Mypy, Pyright, and IDEs agree on\n"
        "# the package-level API exported by nucleation/__init__.py.\n\n"
        "from .nucleation import (\n"
        f"{aliases}\n"
        ")\n\n"
        f"{rendered_overlay}\n"
    )


def main() -> None:
    if len(sys.argv) not in (2, 4):
        raise SystemExit(
            "usage: normalize_stub.py CORE_STUB "
            "[PUBLIC_STUB VENEER_OVERLAY]"
        )
    path = Path(sys.argv[1])
    normalized, nested_count, constant_count, existing_constant_count = normalize(
        path.read_text()
    )
    if (nested_count == 0 or constant_count == 0) and existing_constant_count == 0:
        raise SystemExit("generated stub contained no Diplomat enum wrappers")
    path.write_text(normalized)
    if len(sys.argv) == 4:
        public_path = Path(sys.argv[2])
        overlay_path = Path(sys.argv[3])
        public_path.write_text(
            compose_public_stub(normalized, overlay_path.read_text())
        )
    print(
        f"normalized {nested_count} enum wrappers and {constant_count} constants "
        f"in {path} ({existing_constant_count} already normalized)"
    )
    if len(sys.argv) == 4:
        print(f"composed package-level stub in {public_path}")


if __name__ == "__main__":
    main()
