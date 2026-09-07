"""Select generated bindings using Cargo features and src/bridge/mod.rs gates.

Run at CMake configure time. Both compilation and registration use this same
selection; disabled Rust symbols must never appear in the extension.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path


def expand_features(features: dict[str, list[str]], requested: str) -> set[str]:
    enabled: set[str] = set()
    pending = ['default', *filter(None, re.split(r'[,\s]+', requested))]
    while pending:
        feature = pending.pop()
        if feature in enabled or ':' in feature or '/' in feature:
            continue
        if feature not in features:
            raise ValueError(f'Unknown local Cargo feature: {feature}')
        enabled.add(feature)
        pending.extend(features[feature])
    if 'bridge' not in enabled:
        raise ValueError('NUCLEATION_FEATURES must enable bridge (directly or via bridge-full)')
    return enabled


def cfg_enabled(expression: str, features: set[str]) -> bool:
    expression = expression.strip()
    match = re.fullmatch(r'feature\s*=\s*"([^"]+)"', expression)
    if match:
        return match[1] in features
    if expression == 'target_arch = "wasm32"':
        return False  # This build is a native CPython extension.
    match = re.fullmatch(r'(all|any|not)\((.*)\)', expression)
    if not match:
        raise ValueError(f'Unsupported bridge cfg expression: {expression}')
    parts = []
    depth = start = 0
    for i, char in enumerate(match[2]):
        depth += (char == '(') - (char == ')')
        if char == ',' and depth == 0:
            parts.append(match[2][start:i])
            start = i + 1
    parts.append(match[2][start:])
    values = [cfg_enabled(part, features) for part in parts if part.strip()]
    if match[1] == 'not':
        if len(values) != 1:
            raise ValueError('not() must have exactly one argument')
        return not values[0]
    return all(values) if match[1] == 'all' else any(values)


def disabled_items(repo: Path, features: set[str]) -> tuple[set[str], dict[str, set[str]]]:
    disabled: set[str] = set()
    methods: dict[str, set[str]] = {}
    module_source = (repo / 'src/bridge/mod.rs').read_text()
    for gate, module in re.findall(r'(?:#\[cfg\(([^\n]+)\)\]\s*)?pub mod (\w+);', module_source):
        source = (repo / f'src/bridge/{module}.rs').read_text()
        if gate and not cfg_enabled(gate, features):
            disabled.update(re.findall(r'pub (?:struct|enum) (\w+)', source))
            continue
        # Item-level gates also exist (notably BuildAnimation's GPU methods).
        for item in re.finditer(r'((?:\s*#\[[^\n]+\]\s*)+)pub (struct|enum|fn) (\w+)', source):
            gates = re.findall(r'#\[cfg\(([^\n]+)\)\]', item[1])
            if all(cfg_enabled(gate, features) for gate in gates):
                continue
            if item[2] != 'fn':
                disabled.add(item[3])
            else:
                owners = re.findall(r'\bimpl (\w+)\s*\{', source[:item.start()])
                if not owners:
                    raise ValueError(f'Cannot identify owner of gated method {item[3]}')
                methods.setdefault(owners[-1], set()).add(item[3])
    return disabled, methods


def select_bindings(repo: Path, binding_dir: Path, features: set[str]) -> list[Path]:
    disabled, _ = disabled_items(repo, features)
    return [p for p in sorted((binding_dir / 'src/sub_modules').rglob('*.cpp'))
            if p.stem.removesuffix('_binding') not in disabled]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--features', required=True)
    args = parser.parse_args()
    binding_dir = Path(__file__).resolve().parent
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--no-deps', '--format-version=1', '--manifest-path', str(args.repo / 'Cargo.toml')],
        text=True))
    package = next(p for p in metadata['packages'] if p['name'] == 'nucleation')
    features = expand_features(package['features'], args.features)
    selected = select_bindings(args.repo, binding_dir, features)
    names = {p.stem.removesuffix('_binding') for p in selected}
    source = (binding_dir / 'src/nucleation_ext.cpp').read_text()
    source = '\n'.join(line for line in source.splitlines()
                       if not (m := re.search(r'add_(\w+)_binding\(', line)) or m[1] in names) + '\n'
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'nucleation_ext.cpp').write_text(source)
    _, methods = disabled_items(args.repo, features)
    sources = []
    for path in selected:
        excluded = methods.get(path.stem.removesuffix('_binding'), set())
        if excluded:
            lines = []
            removed = set()
            for line in path.read_text().splitlines():
                method = re.search(r'\.def(?:_static)?\("(\w+)"', line)
                if method and method[1] in excluded:
                    if not line.rstrip().endswith((')', ');')):
                        raise ValueError(f'Expected single-line binding: {line}')
                    removed.add(method[1])
                    if line.rstrip().endswith(';'):
                        lines.append('        ;')
                else:
                    lines.append(line)
            if removed != excluded:
                raise ValueError(f'Could not filter {excluded - removed} in {path}')
            path = args.output / path.name
            path.write_text('\n'.join(lines) + '\n')
        sources.append(path)
    (args.output / 'bindings.cmake').write_text('set(SUBMODULE_FILES\n' + ''.join(f'  "{p.as_posix()}"\n' for p in sources) + ')\n')
    print('Python binding features: ' + ', '.join(sorted(features)))


if __name__ == '__main__':
    main()
