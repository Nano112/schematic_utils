# Nucleation Python API Reference

This documents the surface the published wheel actually ships (`pip install
nucleation`, 0.9.2 and later): the generated binding layer, one compiled
module, no pure-Python wrapper on top. Every signature and output on this page
is verified against that wheel.

> An earlier revision of this page described a "polished" wrapper API —
> `Schematic.new`, `Block`, `Cursor`, `sign()`, property-style accessors —
> designed in `docs/superpowers/specs/2026-05-03-api-upgrade-design.md` for a
> binding architecture the project has since replaced. That wrapper never
> shipped. If you followed the old page and hit `AttributeError`s, this is
> why (issue #3).

## The conventions

Learn these once and the whole surface reads predictably:

- **Everything is a method.** `dimensions()`, `block_count()`, `volume()` —
  there are no properties.
- **Constructors are static and return new objects.** `Schematic.create(name)`,
  `Schematic.load_from_file(path)`, `Schematic.from_litematic(data)`.
  **Importers do not mutate an existing instance** — see
  [Loading](#loading-and-saving) below.
- **Structured returns are JSON strings**, and the method name says so with a
  `_json` suffix: `region_names_json()`, `bounding_box_json()`,
  `changes_json()`. Parse with `json.loads`.
- **Binary returns are base64 strings**, suffixed `_b64`:
  `to_litematic_b64()`, `render_png_b64()`. Decode with `base64.b64decode`.
- **Failures raise `NucleationError`.** Metadata accessors are the deliberate
  exception — they are total: `name()`, `author()`, `description()` return
  `""` when the field is absent, and the numeric getters (`created()`,
  `mc_version()`, …) return `-1`. Absence of an optional field is a blank,
  not an error. (`get_block` on air *does* raise — that is a lookup.)
- **Sizes come back as a `Dimensions` object** with `.x`, `.y`, `.z` fields,
  not a tuple.
- Multi-value string parameters (like `TickSimulation`'s `extra_states`) are
  **semicolon-separated**.

The same API exists in JavaScript (camelCase, `BigInt` for 64-bit ints),
PHP, C and Kotlin — one generated surface for all of them, from
`src/bridge/`.

## Quick start

```python
from nucleation import Schematic

schem = Schematic.create("my_build")
schem.set_block(0, 0, 0, "minecraft:stone")
schem.set_block(1, 0, 0, "minecraft:oak_stairs[facing=east,half=bottom]")
schem.set_block(2, 0, 0, "minecraft:barrel{signal=13}")   # brace shorthands work here

block = schem.get_block(1, 0, 0)
block.name()                              # "minecraft:oak_stairs"
block.properties_json()                   # '{"facing":"east","half":"bottom"}'

schem.get_block(5, 5, 5)                  # raises NucleationError — air and
                                          # out-of-range cells are NotFound,
                                          # there is no None return

schem.save_to_file("out.litematic")       # format from the extension
```

`set_block` and `set_block_from_string` both take full block strings —
properties in `[...]`, and the brace shorthands (`signal=`, `items=`,
`record=`, `{simulate=true}`) documented in
[basics](features/basics.md#content-shorthands).

## Loading and saving

```python
schem = Schematic.load_from_file("build.litematic")      # static, format sniffed

data = open("build.litematic", "rb").read()
schem = Schematic.from_litematic(data)                   # static, returns a NEW Schematic
schem = Schematic.from_data(data)                        # static, sniffs the format
```

**Every `from_*` importer is a static constructor returning a new
`Schematic`.** Python happily lets you call a static method through an
instance, so this line is accepted and does nothing to `target`:

```python
target = Schematic.create("target")
target.from_data(data)        # WRONG: target is untouched; the result was discarded
schem = Schematic.from_data(data)   # right
```

If a freshly "loaded" schematic reports `block_count() == 0`, this is almost
certainly what happened (issue #4).

Available importers: `from_litematic`, `from_schematic`, `from_mcstructure`,
`from_snapshot`, `from_data` (format-sniffing), `from_mca` /
`from_mca_bounded`, `from_world_directory` / `from_world_directory_bounded`,
`from_world_zip` / `from_world_zip_bounded`.

Exporters return base64 strings: `to_litematic_b64()`, `to_schematic_b64()`,
`to_mcstructure_b64()`, `to_snapshot_b64()`, plus version-targeting variants
(`to_schematic_version_b64("1.18")`,
`to_litematic_for_version_json(data_version)`). `save_to_file(path)` writes
bytes directly and infers the format from the extension.

## Dimensions: allocated vs tight

Two different questions, two different methods:

```python
s = Schematic.create("dims")
for x in range(3):
    for y in range(4):
        s.set_block(x, y, 0, "minecraft:stone")

d = s.dimensions()          # allocated region-buffer extent — chunk-sized,
(d.x, d.y, d.z)             # e.g. (66, 66, 1): NOT the content size
t = s.tight_dimensions()    # content extent
(t.x, t.y, t.z)             # (3, 4, 1)
```

`dimensions()` and `allocated_dimensions()` both report the allocated buffer —
storage grows in chunks, so this can exceed the content extent by a wide
margin. **For "how big is this build", use `tight_dimensions()`** (issue #5).
`volume()` is the allocated volume; `block_count()` counts non-air blocks.
Tight bounds follow current content, not mutation history: replacing a boundary
block with air shrinks them, and `tight_bounds_min()` / `tight_bounds_max()`
raise `NucleationError` with code `NucleationErrorCode.NotFound` once the final non-air block is removed.

## Bulk block queries

Three methods exist so a tool never has to pull the whole block list just to
count or rewrite it:

```python
counts = json.loads(schem.count_blocks_json())   # {"minecraft:stone": 1234, ...}
changed = schem.replace_blocks_json('{"minecraft:stone":"minecraft:glass"}')
packed = base64.b64decode(schem.non_air_blocks_packed_b64())
```

`count_blocks_json` tallies non-air blocks by id in one pass.
`replace_blocks_json` applies a from-id to to-id map in place and returns the
number of blocks actually changed; keys match on the id, values may carry
block states. A block that already equals its target is not counted, so a
stone-to-stone map returns 0.
`non_air_blocks_packed_b64` is the compact export: little endian `u32 count`,
then `i32 x, i32 y, i32 z, u16 palette_index` per block, then a `u32` length
and that many bytes of palette JSON. Palette indices are `u16`, so a
schematic holding more than 65,535 distinct non-air block states cannot be
addressed: the method returns an empty string rather than a truncated
palette.

`get_all_blocks_json` still exists and still materialises air, which makes it
`volume()`-sized. Prefer `get_non_air_blocks_json` or the packed export.

## Analysis: fingerprints, diff, auto-stack

These live on their own top-level classes, taking schematics as arguments:

```python
from nucleation import Fingerprint, Diff, Autostack

Fingerprint.is_duplicate(a, b, "exact")          # bool
Fingerprint.footprint_distance(a, b, "shape")    # float, 0.0 = same shape
Fingerprint.compute(schem, "exact")              # digest string

d = Diff.compute(before, after, "exact")
d.distance()                                     # int edit distance
d.added(); d.removed(); d.changed(); d.swapped() # each a Schematic of those cells
json.loads(d.summary_json())

json.loads(Autostack.detect_structures(wall))
# a LIST of detections: [{"mode": "1d", "vectors": [[4,0,0]], "coverage": 1.0, ...}]
bigger = Autostack.resize_1d(wall, 4, 0, 0, 6)
```

The fingerprint presets are distinct equivalence classes — full table and the
`structural` caveat (it sees only solid massing; glass and redstone are
invisible to it, so **it is not an identity**) in
[analysis](features/analysis.md).

## Rendering

```python
from nucleation import RenderConfig, Renderer, ResourcePack

pack = ResourcePack.from_bytes(open("client.jar", "rb").read())
cfg = RenderConfig.create(960, 540)
cfg.set_isometric()
Renderer.render_to_file_with_pack(schem, pack, cfg, "out.png")
png_b64 = Renderer.render_png_b64_with_pack(schem, pack, cfg)   # base64 string
```

Meshing (`MeshConfig`, GLB export, atlases) and the full camera surface are in
[meshing and rendering](features/meshing-and-rendering.md).

## Simulation

```python
from nucleation import TickSimulation, TickSettleMode

sim = TickSimulation.from_schematic(schem, TickSettleMode.InWorld, 0, 0, 0, "")
sim.use_block(0, 1, 0)
sim.run_until_quiescent(400)
json.loads(sim.changes_json())
```

The whole engine — settle modes, reading state, checkpoints, entities,
determinism — has its own manual: [tick simulation](features/tick-simulation.md).
The MCHPRS redpiler surface (`MchprsWorld`, `CircuitBuilder`,
`TypedCircuitExecutor`) is covered in
[redstone simulation](features/redstone-simulation.md).

## Class index

The wheel exports ~85 classes. By feature area, with the doc that covers each:

| area | classes | doc |
|---|---|---|
| Core | `Schematic`, `SchematicSplitResult`, `BlockState`, `BlockPos`, `Dimensions`, `RegionBounds`, `SchematicRegions`, `DefinitionRegion`, `Nbt`, `NucleationError` | [basics](features/basics.md), [world segmentation](features/world-segmentation.md), [block entities & NBT](features/block-entities-nbt.md) |
| Building | `Shape`, `Brush`, `BuildingTool`, `SchematicBuilder`, `Curve3D`, `Geo` | [shapes & brushes](features/shapes-and-brushes.md), [geo](features/geo.md) |
| SDF / fields | `Sdf`, `SdfBounds`, `DistanceField`, `Field3`, `FieldProgram*`, `CellularSdfConfig` | [SDF & fields](features/sdf-and-fields.md) |
| Palettes | `Palette`, `PaletteBuilder`, `Blocks`, `InterpolationSpace` | [palettes & color](features/palettes-and-color.md) |
| Analysis | `Fingerprint`, `Diff`, `Autostack` | [analysis](features/analysis.md) |
| Meshing / rendering | `MeshConfig`, `MeshResult`, `MeshJob`, `Renderer`, `RenderConfig`, `ResourcePack`, `TextureAtlas`, `ItemModel*`, `VideoConfig` | [meshing & rendering](features/meshing-and-rendering.md) |
| Animation | `BuildAnimation`, `AnimationEffect` | [animation](features/animation.md) |
| Simulation | `TickSimulation`, `TickSettleMode`, `MchprsWorld`, `CircuitBuilder`, `TypedCircuitExecutor`, `RedstoneGraph`, `IoLayout*` | [tick simulation](features/tick-simulation.md), [redstone simulation](features/redstone-simulation.md) |
| Worlds / streaming | `WorldStream`, `GeneratedWorldStream`, `WorldGenerator`, `WorldSink`, `WorldChunkView`, `GeneratedChunk*` | [streaming & worlds](features/streaming-and-worlds.md) |
| Segmentation | `WsSegmentJob`, `WsProfile`, `WsRunResult`, `WsPartitionHints` | [world segmentation](features/world-segmentation.md) |
| Voxelize | `Voxelizer` | [voxelize](features/voxelize.md) |
| Scripting | `Scripting` | [scripting](features/scripting.md) |
| Storage | `Store`, `StoreIo` | [storage](features/storage.md) |

For a method-level listing of any class, the wheel is self-describing —
nanobind attaches full signatures:

```python
import nucleation
[m for m in dir(nucleation.Schematic) if not m.startswith('_')]
print(nucleation.Schematic.copy_region.__doc__)
```

The wheel also ships `py.typed` plus generated stubs for the complete compiled
surface. Mypy, Pyright, IDE completion, and API-diff tooling therefore see the
same signatures as runtime `help()`, including the hand-written `design` and
`curation` veneers re-exported by the package.

### Catching native errors

`NucleationError` inherits `Exception`. Its `code` attribute is a
`NucleationErrorCode`; the former `NucleationError.NotFound`-style constants
remain aliases for compatibility. Argument type errors still raise `TypeError`.

```python
try:
    block = schem.get_block(99, 99, 99)
except nucleation.NucleationError as error:
    if error.code == nucleation.NucleationErrorCode.NotFound:
        block = None
    else:
        raise
```
