# nucleation

A high-performance Minecraft schematic engine, powered by a native Rust core. Parse, edit,
diff, fingerprint, and generate schematics from Python.

Policy-driven normalization, material profiles, content inspection, UUID
standardization, bounded decoding, and registry routing are documented in the
complete [transformation-policy guide](../../docs/features/transformation-policies.md).

Wheels are published for CPython 3.12+ (stable ABI) on Linux, macOS, and Windows.
They include a `py.typed` marker and generated `.pyi` stubs for Mypy, Pyright,
and editor completion. The installed package-level `__init__.pyi` explicitly
re-exports every generated native type, followed by the hand-written veneer
types; this keeps package exports equally visible to Mypy and Pyright while
preserving the native classes' type identities.

## Install

```bash
pip install nucleation
```

## Quick start

```python
import nucleation

schematic = nucleation.Schematic.create("demo")
schematic.set_block(1, 2, 3, "minecraft:stone")
print(schematic.get_block_name(1, 2, 3))  # "minecraft:stone"

schematic.save_to_file("demo.litematic")
loaded = nucleation.Schematic.load_from_file("demo.litematic")
```

### Normalize imported content

Preview and apply the same versioned policy contract used by every language
binding:

```python
from nucleation import Schematic, TransformPlan, inspect_transform, apply_transform

schematic = Schematic.open("incoming.schem")
plan = TransformPlan.registry_safe()
preview = inspect_transform(schematic, plan)

if not preview.rejected and not preview.quarantined:
    report = apply_transform(schematic, plan)
    schematic.save("normalized.schem")
```

Inspection never mutates the schematic. Apply is atomic: a rejecting rule
returns `report.rejected == True` and leaves the original unchanged. For only
lossless palette cleanup, use `TransformPlan.canonical()`.

### Split disconnected builds

Keep every meaningful connected machine independent while attaching only tiny,
nearby loose parts:

```python
schematic = nucleation.Schematic.open("combined.schem")
pieces = schematic.split_connected_attach_nearby(
    16,  # components this large always remain standalone
    3,   # tiny parts may attach across at most three empty blocks
)

for index in range(pieces.len()):
    pieces.piece(index).save(f"machine-{index + 1}.schem")
```

Attachment is lossless and non-transitive: fragments cannot form a chain that
recombines otherwise independent builds. Whole-world extraction, including the
Python control plane and remote Store worker, is documented in the
[world-segmentation guide](../../docs/features/world-segmentation.md).

To emit every disconnected component literally, use a zero standalone
threshold. Every component then becomes a core and the gap is ignored:

```python
pieces = schematic.split_connected_attach_nearby(0, 0)
```

### Curate a lossless corpus

Keep raw extraction lossless, then build registry and ranking views with an
auditable policy. Every rejected ID and reason is retained:

```python
from pathlib import Path
from nucleation import (
    CurationPolicy,
    curate_corpus,
    write_registry_archives,
    write_top_owner_archives,
)

policy = CurationPolicy.minima(
    min_blocks=2,          # reject standalone blocks
    min_palette_names=2,   # reject one-material schematics
    name="ore-sanity-v1",
)
corpus = curate_corpus(Path("/data/ore"), Path("/data/ore/curation/ore-sanity-v1"), policy)
write_registry_archives(corpus, Path("/data/ore/registry-import"))
write_top_owner_archives(corpus, Path("/data/ore/top-20-owner-archives"))
```

`CurationPolicy` also accepts declarative `MetricRule` entries over analyser or
catalogue fields and named Python predicates. The policy receives a stable
SHA-256 content ID which is embedded into package indexes and owner manifests.
Changing a filter therefore cannot silently reuse an older curated result.

## What is included

The published wheel contains the core feature set: schematic editing, all schematic formats,
world import and export (including streaming), the schematic builder, the procedural building
tool, definition regions, diff and fingerprinting, autostack, NBT helpers, SDF sampling, and
the in-memory/filesystem store.

Redstone simulation, mesh generation, GPU rendering, and embedded scripting require building
the package from source with the extra cargo features enabled (a Rust toolchain is required):

```bash
git clone https://github.com/Schem-at/Nucleation
cd Nucleation
pip install ./bindings/python
```

The source build defaults to the full feature set (`bridge-full`). Set the
`NUCLEATION_FEATURES` environment variable to choose a different cargo feature list,
for example `NUCLEATION_FEATURES=bridge,simulation`.

## Documentation

- [Python API reference](https://github.com/Schem-at/Nucleation/blob/master/docs/python/README.md)
- [Feature guides](https://github.com/Schem-at/Nucleation/tree/master/docs/features)

## License

MIT

## Building a smaller feature set

Published wheels include `bridge-full`. A source build can select a smaller
Cargo feature set, including transitive dependencies:

```sh
NUCLEATION_FEATURES=bridge pip install --no-binary=nucleation nucleation
NUCLEATION_FEATURES=bridge,rendering,mc-tick pip install --no-binary=nucleation nucleation
```

`bridge` is required. Cargo's normal default features stay enabled. Disabled
features omit their Python types and methods, including animation rendering
methods. Enable `scripting-lua` or `scripting-js` explicitly when needed; a
minimal build requires neither scripting engine. Selection reads the same
Cargo feature definitions and Rust bridge gates for checkouts and sdists.

On Android/Termux, the build explicitly links the interpreter library and
`android`/`log`. Install matching Python development libraries; a missing
interpreter library fails configuration. Ordinary Linux/macOS extensions keep
nanobind's module linkage, without imposing a new libpython dependency.

## Native errors

Catch `nucleation.NucleationError` for engine failures and inspect `error.code`
against `nucleation.NucleationErrorCode` constants. This also applies to
`nucleation.core` and the `open`/`save` convenience methods. Python argument
errors are not converted into engine errors.
