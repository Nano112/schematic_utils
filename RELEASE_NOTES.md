# Nucleation v0.10.24

Fixes the five Python integration reports #36–#40:

- Export NBT compound keys in deterministic order, including nested compounds,
  while preserving list order and array allocations. Transformation history
  checks idempotence against serialized Sponge v3 bytes and records that scope.
- Raise a real Python `NucleationError(Exception)` with a `code` attribute of
  type `NucleationErrorCode`. Existing error constant aliases remain available.
- Build reduced Python feature sets without referencing disabled Rust symbols,
  including individually gated animation methods and their type stubs.
- Use the correct Python module linkage and explicitly link libpython, android,
  and log on Android/Termux. Missing interpreter development libraries fail
  configuration instead of producing an unimportable extension.

Regression coverage includes installed full/minimal/rendering-subset wheels,
serialized artifact reproducibility, exception handling and generated stubs.
Android linker configuration is tested; on-device import validation remains
outstanding.

# Nucleation v0.10.22

The JavaScript bindings now treat wasm32 buffer addresses as unsigned, fixing
negative `Start offset ... is outside the bounds of the buffer` errors when
large imports or exports cross 2 GiB. Signed coordinates and enum values retain
their meaning. String-list allocation also refreshes its memory view after
inner allocations can grow memory. Binding regeneration preserves these fixes.

GLB texture loading borrows embedded buffer/image bytes and consumes decoded
RGBA images instead of making redundant copies. Hollow voxelisation supports
up to 256 million bounding cells (the separate 8-million-surface-block,
16-million-filled-cell and raster-work limits remain). A Westminster scan with
an 8K texture exported at height 384: 588 × 384 × 1164, 7,829,382 blocks.

Sponge loading decodes directly into the region's final index storage, avoiding
two additional full-volume arrays. The Westminster reload's wasm memory peak
fell from approximately 3.70 GiB to 1.74 GiB. Declared block counts and varint
lengths are checked before accepting the decoded data.

`Schematic.getChunkNonAirBlocksJson` supports bounded preview loading with
block-state properties and negative coordinates intact. It reads intersecting
region cells directly and omits all air variants, avoiding whole-volume scans
and enormous air-filled horizontal slabs. Queries cap visited cells and JSON
output; 16³ sections are recommended.

# Nucleation v0.10.21

Model material import now distinguishes opaque surfaces, alpha cutouts and
transmissive glass. GLB loading applies base-colour factors and vertex colours
in linear light, samples emissive and transmission textures with their own UV
sets, retains emissive colour under baked lighting, and applies default-pose
skin transforms. Only the selected/default scene is imported. Animation,
refraction, cast shadows and metallic/roughness rendering are not baked.

`BlockPalette::new_materials` / `Palette.materials()` include opaque building
blocks and full glass. `for_material` / `forMaterial` restrict any custom palette
to opaque or glass candidates before nearest-colour matching or dithering;
exclusions stay respected. Missing material candidates produce an actionable
error in the configured voxelizer. Neutral clear glass uses white transmission
colour rather than its decorative texture streaks. Existing explicit block
replacement operations still override the match.

Texture sampling now interpolates in linear light and respects repeat, mirrored
repeat and clamp wrapping. This intentionally changes the textured-cube golden
colours (geometry unchanged); the three geometry/shading baselines are retained.


Model imports can target height, width, depth, or the longest side while
preserving transformed mesh proportions. The new `VoxelModel` API parses GLB
or OBJ once, returns an allocation-checked size plan, and exports an
origin-aligned schematic. Optional directional Lambert lighting shades texture
colours before palette matching; strength zero preserves the original colours.

Hollow imports through this API use a sparse triangle surface raster rather
than a dense triangle grid and interior fill. This makes large landscape
surfaces practical: a synthetic 512 × 384 × 512 sloping surface exported
654,336 blocks through WASM during validation. The output buffer remains dense;
explicit volume, dimension, surface-block and raster-work limits reject oversized
requests. The legacy voxelizer entry points retain their existing behaviour.

All generated bindings expose the configured importer. PHP can load GLB bytes
via base64 to avoid expanding every byte into an integer array. Regression tests
cover axis sizing at height 384, transformed GLB textures, lighting direction,
zero-strength identity, hollow/filled interiors, sparse/dense surface membership,
and bridge errors. Browser/PHP integration checks agree on the shaded output.

# Nucleation v0.10.20

`Region::merge` now refreshes its occupied bounds after combining block data.
Previously, merging into an empty region or adding blocks outside the first
region's occupied area left those bounds stale. A subsequent `to_compact()` could
discard the new blocks even though `count_blocks()` reported them.

The fix preserves merge precedence: air in the incoming region still does not
erase existing blocks. The higher-level schematic merge already refreshed bounds
in its precedence-aware path; this release also fixes callers of `Region::merge`
itself. Regression tests cover disjoint regions, empty destinations, negative
region sizes, air overlap, and Litematica v7 to Sponge conversion.

# Nucleation v0.10.19

This release also carries the world-segment changes published as 0.10.17 and 0.10.18 from the ingestion branch (observable extraction, explicit source gap receipts, no implicit selection boundary certification), which had no notes of their own.

**Mesh voxelization is no longer quadratic in the volume.** Filling a
voxelized mesh used to call `MeshShape::normal_at` for every solid voxel
whatever the brush did with the value, and each of those calls ran an
expanding ring search over the triangle grid with a fresh allocation, so the
cost grew as the sixth power of the target size. Three changes fix it.
`Brush::uses_normal` lets a brush say it ignores the surface normal, and the
seven brushes that do (`Solid`, `Color`, `Linear`, `MultiPoint`, `Point`,
`Bilinear`, `Field`) now cost nothing to shade. `MeshShape` precomputes a
triangle id per voxel once, next to the mask it already builds, with one
rayon pass over the triangles and one BFS to hand ids inward, so `normal_at`
and `surface_color` are array lookups. `voxelize_textured` samples colours in
parallel, memoises the palette search on the exact colour and resolves each
palette index to a block once instead of cloning a `String` per voxel.

| case (5,000 triangle sphere, BoxTextured cube) | 0.10.16 | 0.10.19 |
| --- | --- | --- |
| solid fill, size 32 | 424 ms | 1.66 ms |
| solid fill, size 64 | 19.0 s | 8.44 ms |
| solid fill, size 128 | not measured | 75.1 ms |
| shell fill, size 32 | 8.44 ms | 1.80 ms |
| shell fill, size 64 | 23.0 ms | 3.79 ms |
| shell fill, size 128 | not measured | 12.8 ms |
| textured cube, size 32 | 391 ms | 9.01 ms |
| textured cube, size 64 | 29.1 s | 69.0 ms |
| textured cube, size 128 | not measured | 2.28 s |

Measured on the build host (16-core x86_64 server), medians, with `cargo bench
--features bridge,voxelize --bench voxelize_bench`. The 0.10.19 column is one
fresh run of all three cases at all three sizes on the released code, so it
supersedes the per-task figures recorded while the branch was in progress.
Size 128 was never benchmarked on unchanged 0.10.16 code, because on the old
N^6 trend one sample took roughly half an hour: the one 0.10.16 size-128
number that does exist is a solid fill timed by hand at 1,578 s, which is
where this work started. The output is unchanged: a committed golden fixture
pins the sha256 of the sorted block list for the solid sphere, the shell
sphere and the textured cube at size 32, and it holds byte for byte across
all three changes. A fourth case, a wool-palette `ShadedBrush` fill of the
same sphere, is pinned fresh at this release: it is the one that runs
`normal_at` on curved geometry, so it guards the new triangle-id field
itself.

**Bulk block queries on the bridge.** `Schematic.count_blocks_json` tallies
non-air blocks by id in one pass, `replace_blocks_json` applies a from-id to
to-id map in place and returns how many blocks changed, and
`non_air_blocks_packed_b64` exports positions and palette indices as a
compact little endian blob with the palette as length prefixed JSON. Tools
that used to pull `get_all_blocks_json` just to count or rewrite materials no
longer have to; that method still exists, still materialises air, and its
documentation now says so. All three treat `minecraft:air`, `cave_air` and
`void_air` alike as air.

**`Brush::uses_normal`.** A brush trait method, default `true`, that a brush
overrides to say it never reads the surface normal or colour handed to it.
The seven brushes that always return the same value regardless of shading
input (`Solid`, `Color`, `Linear`, `MultiPoint`, `Point`, `Bilinear`, `Field`)
override it to `false`, letting the fill loop skip the `normal_at` call
entirely for them.

**Two behaviour changes to check before upgrading.**

`get_non_air_blocks_json` now excludes `minecraft:cave_air` and
`minecraft:void_air` as well as `minecraft:air`. Every "non-air" method in the
library shares one `is_air` definition covering all three, which is what the
name always claimed; before this release only `minecraft:air` was filtered, so
a schematic carrying cave or void air handed those blocks to the caller. A
caller that was relying on receiving them (to rebuild a region's exact cell
contents, say) should use `get_all_blocks_json` instead.

`fill_sdf_function` no longer calls the caller's `normal` closure when the
brush's `uses_normal()` is `false`. This is the `Brush::uses_normal` gate
applied to the SDF path, and it reaches the Python FFI:
`nucleation_python_fill_sdf_function` will not invoke a Python normal callback
for a fill whose brush ignores normals. The blocks placed are unchanged, since
the value was discarded anyway, but a callback with side effects (counting
calls, logging, mutating state) will see fewer of them, and one that raised on
purpose will no longer be reached.

`replace_blocks_json`'s return value is also now strictly the number of blocks
that changed: a block already equal to its target is skipped, so a map that
rewrites an id to itself returns 0 rather than the number of matches.

# Nucleation v0.10.16

**Mesh voxelization in the browser.** The wasm build now ships the `voxelize`
feature, so `Voxelizer.shape_from_obj`, `Voxelizer.shape_from_glb` and
`Voxelizer.schematic_from_glb_textured` are exported from `nucleation.wasm` and
callable from the npm package. The generated JS bindings already described this
surface; only the wasm binary was missing the symbols, so an OBJ/GLB import had
to round-trip through a server. Its dependencies are wasm-clean (`gltf` with
default features off, `image` decoding bytes handed to it), so nothing else in
the package changes.

# Nucleation v0.10.15

**Immutable, incremental world snapshots.** `segment_world index` stores Anvil
regions by content hash and produces deterministic manifests with semantic chunk
hashes. Repeated backups reuse unchanged region indexes and stored objects;
snapshot-backed extraction verifies object integrity without needing the original
world. Unreadable regions can be recorded explicitly for operator review.

**Observable, coverage-certified extraction.** The worker emits NDJSON progress
and seals successful snapshot-backed runs with a versioned completion report.
Extraction rejects artificial boundary cuts instead of silently producing partial
builds, while complete hard partitions remain valid. The worker is also available
as a Cargo binary, including in the crates.io source package.

**Build-animation anchors and cross-binding parity.** `BuildAnimation` records
named anchors (`add_anchor`, `add_anchor_to_group`, `anchors_json`) that every
frame reports in world space after the group's pose, for hotspots and labels
that land with their block. The beacon and crafting-nook builds are pinned as
native fixtures and verified against the WASM/JS and Python engines by
`tools/verify-build-animation.sh`. `BuildAnimation.to_animated_glb_b64(pack, fps)`
exports the build as an animated GLB — named group nodes with textured meshes,
TRS keyframes, anchor child nodes, and `extras.nucleation` for opacity / tint /
emissive and camera tracks — so a build plays in any glTF viewer. `to_schem_b64` / `to_litematic_b64` export the finished build as a Sponge schematic or a Litematica file.

# Nucleation v0.10.14

**Version-matched world extraction workers.** Every native release archive now
ships the production `segment_world` executable beside the shared library,
generated headers, and language bindings. Distributed consumers can install one
release atomically instead of compiling an architecture-specific helper on each
worker. The packaged driver retains bounded directory/archive reads, SSH-backed
Store input, deterministic provenance, partition/floor handling, disconnected
component splitting, and support-block preservation.

# Nucleation v0.10.13

**Policy-driven normalization and registry ingestion.** Schematics can now be
processed through versioned, ordered transform plans with lossless palette
canonicalization, named material profiles, recursive content policies, and
fine-grained deterministic or random UUID rewriting. Inspection runs the exact
plan on a clone; application is atomic and appends deterministic transformation
history without altering source provenance.

**Untrusted-input boundaries.** Bounded readers enforce compressed and
decompressed byte ceilings, dimensions, volume, region/palette/entity counts,
and recursive NBT budgets. The storage-backed `RegistryPipeline` streams from
any `Store`, routes accepted/quarantined/rejected objects, persists stable audit
reports, and exposes declarative hooks that may only escalate a decision.

**One contract across seven languages.** The generated bindings expose the
shared versioned JSON plan and report surface, with an idiomatic typed Python
facade for plans, reports, limits, material profiles, content rules, UUID
policies, and registry configuration. The README and feature guide now include
an end-to-end quick start, complete field/default reference, action semantics,
registry output behavior, and an operational deployment checklist.

**HDL compatibility.** PLA route scheduling now keeps recycled channels clear
of incoming landing tails, preventing a support block from silently capping an
older route at a stage boundary. The Ubuntu-packaged Yosys 0.33 comparator is a
checked-in regression and passes all 256 input combinations in both compilers.

# Nucleation v0.7.0

**Generated worlds.** A new `world_generation` module turns chunk sources into
whole worlds without ever leaving Rust. `WorldGenerator` builds from an SDF
volume, a hashed-cell motif scattered once per deterministically transformed
cell, projected building footprints, or an ordered composite of any of those.
`stream()` walks an inclusive chunk rectangle lazily in canonical region-major
order, and each `GeneratedChunk` carries coverage plus source/version
provenance before handing its payload to the existing `WorldChunkView` — so
generated worlds flow straight into the Anvil path already there. The bindings
expose concrete source constructors rather than host callbacks, which keeps
SDF evaluation and block placement off the FFI boundary entirely.

**Portable field programs.** `FieldProgram` and `FieldProgramBuilder` add a
sandboxed stack-based VM for field graphs that can cross a language boundary as
JSON. Slots, instruction count, repeat depth, repeat iterations, and total
dynamic steps are all capped, so a program arriving from an untrusted caller
cannot run away. `Sdf.from_program` turns one into geometry.

**Field3.** A scalar field with no distance semantics, so geometry
(`Sdf.offset_by_field`) and materials (`Brush.field3`) can share one field
without it being reinterpreted as a signed distance. `output_range` reports the
analytically proven bounds as a single checked value.

**The SDF bridge is now complete.** It grew from a 4-method stub to the full
typed surface: 23 IQ primitives, boolean/smooth/domain-warp operators,
transforms, evaluation, and JSON round-trip — every constructor validating its
arguments. `Brush.field_sdf` and `Palette.gradient_ids_between_blocks_json`
round out the material side.

**Error fidelity.** World-generation failures on a well-formed request now
report a new `NucleationError::Generation` instead of collapsing into
`InvalidArgument`, so bindings can tell "you asked for something impossible"
apart from "the generator broke". `Sdf.bounds` returns `NotFound` for an
unbounded graph, per the bridge's error convention.

**Breaking.** `SdfShape::with_bounds` returns `Option<Self>`, validating its
bounds rather than trusting them. On the bindings, `Field3.output_min` and
`Field3.output_max` are replaced by `Field3.output_range`, which errors when no
range can be proven instead of silently returning NaN — check any call site
that fed those into a gradient's bounds.

# Nucleation v0.5.1

**USDZ byte accessor.** `MeshResult` gained a `create_usdz` constructor when
the bindings migrated to the Diplomat bridge, but the only byte accessor left
on it was `glb_data_b64` — meshing as USDZ still only ever handed back GLB
bytes. This restores the USDZ byte path lost in the bindings migration:
`MeshResult.usdzDataB64()` alongside the GLB accessor, generated across all
six bindings.

# Nucleation v0.5.0

**World segmentation.** A new `world_segment` module (feature `world-segment`)
turns a whole world save — a directory or a streamed `.tar.gz` backup — into
individual builds, each a normal schematic plus a provenance envelope recording
where it came from. The natural ground is learned from the world itself
(`WorldProfile::derive`) and subtracted so terrain stops gluing structures
together; morphological closing keeps a machine and its floating wiring as one
build; clusters that cross region boundaries are stitched back together by a
merge that is associative, commutative, and idempotent (property-tested), so
partial results combine in any order. Every build is tiered
`Confident`/`Probable`/`Debris` from explainable signals — debris is labeled,
never deleted.

The pipeline is deterministic end to end: no clock, no randomness, no
iteration-order dependence — the same bytes and configuration produce
byte-identical builds and provenance, verified by re-running a real 845-region
(1.6 GB) world to identical output. That world became ~4,500 addressable builds
in about half an hour through the streaming runner
(`WorldSegmenter::run_streaming`), which emits builds one at a time instead of
holding them all.

Optional **partition hints** (any set of named boxes — a plot grid, districts)
guarantee no build ever spans a boundary, with per-partition floor detection for
parcelled worlds where each owner floors their plot differently. Re-extracting a
newer snapshot matches builds by overlap (`match_snapshots`), so an edited build
keeps its `StableBuildId` and simply gains a new fingerprint — splits and merges
are labeled as such. The full story, calibration knobs, and gotchas are in
[the world segmentation guide](docs/features/world-segmentation.md); the runner
is exposed across the generated bindings (`WsSegmentJob`, `WsPartitionHints`,
`WsProfile`, `WsRunResult`).

# Nucleation v0.4.1

**Python file I/O compatibility.** The generated Python `Schematic` binding
once again exposes the established `Schematic.open(path)` and
`schematic.save(path, format=...)` convenience API, including `pathlib.Path`
support and Python's normal `None` return for successful saves. The lower-level
`load_from_file` and `save_to_file` methods remain available. Release wheels now
exercise real inferred-format and explicit-format save/open round trips so this
surface cannot disappear unnoticed again.

---

# Nucleation v0.4.0

**General Java structure SNBT naming.** The public format key and Rust module
are now `structure_snbt`, with `StructureSnbtFormat`, `is_structure_snbt`,
`from_structure_snbt`, and `to_structure_snbt`. The short-lived
`gametest_snbt` name has been removed rather than retained as an alias: this is
the general textual Java structure source format used by mod/data-pack tooling,
guides, and GameTest suites, not a GameTest-only container.

---

# Nucleation v0.3.19

**Construction animations and operation-aware transforms.** `BuildAnimation`
now records grouped edits, region and whole-schematic rotations, translations,
flips, stamps, receipts, exact lattice transforms, pivots, and gizmos while
keeping authoritative schematic state separate from presentation. The API is
available through the generated language bindings, with deterministic region
precedence, explicit-air masking, tight transformed bounds, and transactional
failure behavior.

**Java structure SNBT I/O.** Textual `.snbt` structure sources now auto-detect,
import, export, and semantically round-trip block states, typed block-entity NBT,
entities, dimensions, and `DataVersion`. MCA detection now validates the region
header and chunk record structure instead of misclassifying large text files.
Import/export allocation limits fail closed on hostile dimensions, and the
implementation is exercised against all 33 pinned Lithium GameTest structures.

**Documentation and correctness.** The README is now backed by executable,
downloadable examples for construction animation, regions, transforms,
stamping, overlap precedence, and all supported schematic containers. Region
transforms rebuild tight bounds, X/Z stair rotations preserve valid Minecraft
states, block-entity content shorthands are stricter, and sampled-curve work is
bounded against pathological inputs.

**Release packaging.** Python artifacts now include a self-contained source
archive, build Linux wheels against the manylinux 2.28 glibc floor, and pin the
Apple Silicon wheel to macOS 11 compatibility instead of inheriting the runner's
host OS version.

---

# Nucleation v0.3.18

**A material system over any geometry.** New `DistanceField` primitive:
`DistanceField.from_schematic(build)` runs a distance transform over the
occupancy and answers `depth(x,y,z)` (blocks below the surface), `slope(x,y,z)`
(the upward component of the surface normal), and `normal_json(x,y,z)`. An SDF
shape already exposes depth and normal for free; this recovers both for any
build, an imported schematic, a voxelized model, or map data, so materials can
key on depth and slope over arbitrary geometry: paint grass on the flats and
stone on the steeps, weather a temple with moss, snow, patina, strata, ambient
occlusion, or edge wear. Available across all seven language bindings.

**Meshing fix.** A bare block id now meshes as its default state instead of the
all-false variant, so multi-face blocks render correctly (e.g.
`red_mushroom_block` shows its red cap, not the pale inside faces) everywhere:
rendering, GLB/USDZ export, and color matching.

---

# Nucleation v0.3.17

**Patterns as fields.** A `cells` SDF node adds Worley / Voronoi noise to the
field language, so cellular patterns compose into the SDF like any node and
drive geometry (`f1` / `f2` / `f2MinusF1` distance fields, or a per-cell
`value`; thresholded, `f2MinusF1` carves a foam). And `Brush.field(field_json,
stops, colors, lo, hi, space)` colors each voxel by evaluating any field through
a gradient, so the same language drives color: a `cells value` field paints a
Voronoi mosaic, an fbm field a marble, a coordinate expression a stripe. Voronoi
is one field; the same brush and node take any of the others. All seven
bindings.

---

# Nucleation v0.3.16

**Nucleation as a custom world generator / processor.**
`WorldChunkView.from_schematic(schematic, cx, cz)` is the write-side twin of
`to_schematic`: fill a schematic with *any* tool — shapes, SDF, brushes, OSM
footprints, a heightmap, noise — clip it to a chunk, and stream it straight to
a playable world with `WorldSink`. Run the bridge the other way (`WorldStream`
→ `to_schematic` → transform → `from_schematic` → `WorldSink`) and it is a
constant-memory world filter. Intersecting the fill with each chunk means the
source is only evaluated inside the chunk being written, so worlds of any size
generate in flat memory. All seven bindings.

---

# Nucleation v0.3.15

**Storage: sensible default format.** `StoreIo.save` / `Store.save_schematic`
no longer error when the key or path has no recognizable format extension —
they default to litematic. The written bytes are self-describing, so `open`
reads them back by content regardless of the key, and a bare store key like
`builds/castle` now round-trips instead of surfacing an opaque error.

---

# Nucleation v0.3.14

**Geodata is first-class.** The moves behind the README's mountain and city
showcases are now real, network-free API — you fetch and project, they build
the blocks:

- `Shape.polygon_prism(polygon_json, y_min, y_max)` — extrude a closed 2D
  footprint (even-odd fill, concave-safe) between two Y levels. The primitive
  for building footprints, lake outlines, plot fills.
- `Geo.extrude_footprints(buildings_json, base_block, name)` — OSM-style
  footprints → a massed city, stamped tallest-wins per column on a ground slab.
- `Geo.heightmap_terrain(heights_json, width, surface_blocks_json,
  subsurface_block, surface_depth, name)` — an elevation grid → terrain
  columns, with a single surface block or one per column for elevation/slope
  bands (snow/scree/meadow).

Both README recipes now run through these (the Matterhorn render is
byte-identical to the hand-rolled version), and a new showcase streams the
2.4M-block Financial District straight out to a playable Minecraft world.
Available in all seven language bindings.

---

# Nucleation v0.3.13

**Surface-only mesh voxelization.** `Voxelizer.shape_from_glb` /
`shape_from_obj` / `schematic_from_glb_textured` now accept a **negative**
`shell` value, which voxelizes an open sheet as a skin |shell| blocks thick
with **no parity interior fill**. Open surfaces that fold back on themselves —
a road ribbon that dips into a valley or crosses over itself — no longer trap
the inside/outside test into filling the enclosed pockets. (Rust:
`MeshShape::with_surface_shell`.) Positive `shell` is unchanged (parity solid
+ shell); `0` is still pure parity.

This is the last piece of the README Rainbow Road: the course now voxelizes as
a clean rainbow ribbon, dips and overlaps and all.

---

# Nucleation v0.3.12

**Fix: `region_bounding_box_json` now reports tight content bounds.** It was
returning the region's internal storage box, which `expand_to_fit`
over-allocates by up to 64 blocks per axis — so a region holding blocks at
(0,0,0)..(3,2,3) reported `[0,0,0,67,66,67]` instead of `[0,0,0,3,2,3]`. It
now uses the tight min/max of placed non-air blocks (empty regions fall back
to the allocated box).

Docs: the README was restructured around author → read/process → analyze →
data → integrate, and the read/stream/regions/NBT/scripting/storage surface
that was previously one buried section is now first-class — including a new
chunk-streaming visualization, a multi-region before/after, and a
block-entity vault. Snippets for chunk iteration, regions/transforms,
NBT, and storage are verified in `docs/readme-snippets/`.

---

# Nucleation v0.3.11

**`Palette.closest_block_dithered(r, g, b, x, y, z)`** — the per-pixel
entry point for image mapping and pixel art: position-aware ordered
dithering between the two nearest palette blocks, deterministic. It powers
the README's new showcases: four public-domain paintings as block art
(Starry Night, Sunflowers, The Great Wave, Girl with a Pearl Earring), a
rotating voxel Earth whose blocks are re-picked per frame by luminosity
through a day/night terminator, the Matterhorn from elevation tiles, and
Wall Street from OpenStreetMap.

---

# Nucleation v0.3.10

**Ordered dithering.** `Palette.dithered()` makes every brush alternate
between the two nearest blocks per voxel (4x4 Bayer threshold on the
target's position along the Oklab segment between them) — deterministic,
and gradients stop banding. SDF gradient fills take `"dither": true` for
the same effect between ramp steps.

**The voxelizer got ~1000x faster at fills.** Bulk solves now run three
scanline parity sweeps (one ray per column per axis, rayon-parallel,
majority vote — same robustness as the per-voxel test) plus per-triangle
shell rasterization, cached as a bitset on the shape. A 6k-triangle
Mario Kart course: 107s → 0.1s at size 200; a 515-block-long
voxelization solves in 1.5s.

---

# Nucleation v0.3.9

**Voxelize 3D models.** New `voxelize` feature (in `bridge-full` and the
WASM build): load GLB (node transforms, embedded textures) or OBJ into a
`MeshModel`, and use it as a first-class building `Shape` —
inside/outside via triangle-parity ray casting (three-axis majority vote,
grid-accelerated), normals from the nearest triangle so lighting brushes
just work, and an optional `shell` distance that closes thin-walled and
hollow geometry (the canonical Utah teapot is a double-walled vessel —
parity alone is faithful to that). Texture projection maps each voxel to
the palette-closest color of its nearest surface point (barycentric UVs,
bilinear sampling): `Voxelizer.schematic_from_glb_textured`.

**Spotlight brush.** `Brush.spotlight(pos, direction, cone_angle, color)`
— Lambert term from the surface normal times a smooth cone falloff,
snapped to any palette. Point it at a voxelized teapot through the
grayscale ladder and you get film-noir ceramics.

---

# Nucleation v0.3.8

**The basics got simple.** `load_from_file` auto-detects the format from
file contents (previously Litematic-only — it couldn't open a `.schem`);
`save_to_file` picks the format from the extension. The explicit
`save_to_file_with_format` remains.

**Palettes from pure color logic.** `PaletteBuilder` gains
`lightness_between(min, max)`, `chroma_below(max)`, and
`color_near(r, g, b, distance)` — filters over each block's *measured*
Oklab color, composable with the tag/kind/flag filters. And
`Blocks.by_color(r, g, b, max_distance)` queries the whole block database
by color, nearest first.

**SDF trees are Shapes.** `Shape.sdf(json)` (and `sdf_bounded`) turns any
distance-field tree — smooth unions, noise, all of it — into a first-class
building shape: fillable with every brush, combinable with other shapes,
usable in masked fills. Normals come from the field gradient, so the
shaded brush shades smooth blends continuously. The terrain sampler and
the building system now share one geometry language.

---

# Nucleation v0.3.7

- **`Palette.ramp_ids(start, end, steps)`** — ask for pure white → pure
  black in N steps and the engine picks N *distinct* blocks forming the
  smoothest ramp the palette allows: targets evenly spaced along the Oklab
  line, blocks assigned by a minimum-cost monotonic matching (unlike
  `gradient_ids`, which snaps per step and repeats). In every binding as
  `ramp_ids_json`.
- **`RenderConfig.set_sphere_fit(true)`** — rotation-invariant camera
  framing: turntables hold a constant distance instead of pulsing with the
  model's silhouette.
- **`Palette.grayscale()` is now data-driven** — opaque full cubes with
  near-neutral *measured* color (low Oklab chroma) instead of name
  substrings, which caught cream sandstones and patterned glazed
  terracottas while missing neutral blocks named otherwise.

---

# Nucleation v0.3.6

Fixes surfaced by making the library render its own README
(every image at https://github.com/Schem-at/Nucleation is now generated by
`tools/readme-media/generate.py` through the Python binding):

- **`Torus.parameter_at` fixed** — the ring angle was measured from raw
  world components of the radial projection, which is identically zero on
  one axis for the default y-up torus, so parametric `curve_gradient`
  fills collapsed to two colors. The angle is now measured in a proper
  in-plane basis; gradients sweep the full ring on any torus axis.
- **`RenderConfig.set_zoom` is now a real zoom** — it used to scale the
  camera distance (larger = further away). Now larger = closer
  (2.0 = twice as close, 0.5 = twice as far), in both perspective and
  orthographic projections. Invert your values if you used the old
  behavior.
- **JS bindings drop the filesystem methods** — `loadFromFile`,
  `saveToFile`, and `saveToFileWithFormat` always threw `Io` under WASM
  (no filesystem); they are no longer in the JS typings. Use
  `fromData(bytes)` and the `to*B64()` exporters.
- **`Sdf.schematicFromSdfAuto(sdf, rules)`** — auto-bounds overload; no
  more six placeholder arguments when the SDF tree bounds itself.
- **`Palette.grayscale()` is full-cubes only** — the name match also
  caught panes/stairs/walls (e.g. `light_gray_stained_glass_pane`),
  which rendered as holes when gradients snapped to them.

---

# Nucleation v0.3.5

**Linux release libraries now target glibc 2.35 (was 2.39).** The native
`.so`s (PHP FFI) and the JVM jar's bundled linux natives (JNA) are built on
ubuntu-22.04 instead of ubuntu-latest, so they load on older-glibc deploy
targets — e.g. Debian bookworm (glibc 2.36), where the v0.3.x libs failed
`FFI::cdef` with "GLIBC_2.38 not found". No API changes.

---

# Nucleation v0.3.4

**meshing and rendering are on the crates.io crate again.** schematic-mesher
is published to crates.io (0.2.0), so the dependency is now dual
version+git: local/git builds use the pinned rev, `cargo publish` keeps the
versioned crate. The published crate no longer strips meshing/rendering —
only `simulation` (MCHPRS) stays git-only.

**The nanobind pin is gone.** The Python wheel accepted only
`nanobind ==2.12.0` because the generated dealloc shim reached into
nanobind's private struct layout (broken in 2.13). The diplomat fork's
nanobind backend now uses nanobind's public low-level instance API, so the
pin is `>=2.12,<3` — verified building and running a create/drop
destruction stress against both 2.12 and 2.13.

**Block data polish** (all from official 26.2 sources):

- `default_state` is now populated for all 1,196 blocks (was empty) — the
  Blocks query API returns real default property maps
- Tile-entity classification comes from the `block_entity_type` registry
  (186 blocks, was 42 by substring) — signs, banners, skulls, shelves, ...
- Light emission uses per-block emit-light data, not name guessing
- Mushroom blocks classify as full cubes

**Automated data refresh**: a weekly workflow checks Mojang's manifest and
opens a PR (regenerated data + new-blocks diff) when a new Minecraft release
ships.

---

# Nucleation v0.3.3

**Block semantics from official data, queryable everywhere.** The data
pipeline now extracts three new facets straight from the Minecraft 26.2 jars:
definition kinds + base-block links (Mojang's own variant data: oak_stairs
knows it is a `minecraft:stair` of `oak_planks`), fully-resolved vanilla
block tags (265 tags — wool, planks, mineable/pickaxe, ...), and
model-derived full-cube geometry for every block. Substring guessing is
retired: `full_blocks_only`, `exclude_transparent`, and the technical-block
exclusion are all metadata-driven now.

New in every language binding:

- **`Blocks` query API**: `get(id)` (kind, base block, tags, geometry,
  color, properties), `byTag`, `byKind`, `variantsOf(base)` (the whole
  family: stairs/slab/fence/button/...), `states(id)` (every property
  combination), `tags()`, `ids()`, `count()`
- **`PaletteBuilder.tag(...)/.excludeTag(...)/.kind(...)`** — palettes from
  real tags and kinds instead of keywords
- **Masked fills**: `BuildingTool.fillOnlyAir(...)` and
  `fillReplacing(shape, brush, targets)` for non-destructive edits
- **SDF gradient materials**: fill rules accept `gradient` (palette +
  from/to color along y or depth, or a lightness ramp) — terrain with block
  gradients from pure JSON
- **Scripting**: `palette_gradient_ids`, `palette_block_ids`,
  `palette_closest_block` in the Lua and JS engines

Java↔Bedrock mappings refreshed from GeyserMC's new NBT format, now at
**Java 26.2 parity**: 32,366 blockstate mappings, full coverage including
the 26.2 blocks, zero fallbacks.

---

# Nucleation v0.3.2

**The block database now lives inside nucleation, current to Minecraft 26.2.**
blockpedia is no longer an external dependency: block facts, Java↔Bedrock
mappings, and texture-derived colors ship in-tree (gzipped, ~330 KB) and are
generated at build time. Data targets **Java 26.2** (Mojang's new versioning),
extracted with Mojang's own data generator — 1,196 blocks including the new
cinnabar/sulfur families — with colors computed from the 26.2 client jar's
default textures (98.4% coverage, plains-biome tints applied). Refreshing for
a future release is two commands with no code changes
(`refresh-block-data` + `fetch-texture-colors`, both `--features mc-data-refresh`).

Palette upgrades for value→block workflows (heatmaps, fractals, pixel art):

- `Palette.sortedByLightness()` — any palette as a dark→light ramp
- `Palette.gradientIdsJson(r1,g1,b1, r2,g2,b2, steps)` — exactly N block ids
  sampling an Oklab gradient snapped to the palette; index by intensity
- `Palette.wood()` — the planks family, a natural wood ramp
- Default palettes exclude technical blocks (portals, fluids, fire, ...)

Also: the npm wasm now includes **simulation and meshing** (in-browser
redstone simulation works again); local Python wheel builds no longer trust a
stale rust lib.

---

# Nucleation v0.3.1

**Fixes broken v0.3.0 native release artifacts.** The v0.3.0 per-platform
libraries were built with the core `bridge` feature only, so every
meshing/simulation/rendering export was missing — PHP's eager `FFI::cdef`
could not even bind the release zip's own bindings. All native artifacts
(platform zips, JVM jar natives) now ship the full `bridge-full` surface,
matching the wheels, and CI now installs and exercises every wheel and the
assembled jar (including a simulation-symbol check) before anything ships.

Also in this release:

- **First-class palettes** in every language: `Palette` (solid / structural /
  decorative / concrete / wool / terracotta / grayscale presets, custom
  palettes from a JSON block-id list, closest-block lookup),
  `PaletteBuilder` (blockpedia filter flags + keyword include/exclude), and
  `Brush.setPalette(...)` on all color/gradient brushes — bindings are no
  longer locked to the built-in all-blocks palette. Default palettes now
  exclude technical blocks (portals, fluids, fire, piston internals).
- **JVM jar is multi-platform**: natives for linux x64/arm64, macOS
  x64/arm64, and Windows x64 are bundled in JNA layout (previously linux
  x64 only).
- **crates.io publishing works again**: the published crate ships without
  the git-only features (`simulation` — MCHPRS; `meshing`/`rendering` —
  schematic-mesher); use the git dependency for those.

---

# Nucleation v0.3.0

**Breaking: every language binding is now generated from a single source of truth.**

The four hand-written binding layers (C FFI via `#[no_mangle]` externs, WASM via
wasm-bindgen, Python via pyo3, JVM via hand-written JNI) and the experimental
ext-php-rs extension are gone, replaced by Diplomat-generated bindings for
C, C++, JS/WASM, Kotlin (JNA), Python (nanobind), and PHP (ext-ffi) — all generated
from `src/bridge/` by `tools/gen-bindings.sh` into `bindings/`, and regenerated +
diffed in CI so they can never go stale. The regex parity linters are deleted;
coverage vs the old 544-function C surface is enforced by
`tools/check_bridge_coverage.py` against a frozen baseline.

API changes to be aware of:
- One unified error model: every fallible call returns/raises `NucleationError`
  (12 variants). The thread-local `schematic_last_error`, per-function int/null
  sentinels, and error-string returns are gone.
- Constructors are `create`/`from_*`; accessors drop `get_`/`set_` prefixes
  (per-language casing applies, e.g. `getBlockName` in JS/PHP).
- Domain methods moved off the `Schematic` god-object onto their own types
  (`Diff`, `Fingerprint`, `Autostack`, `StoreIo`, `Renderer`, meshing types,
  `SchematicRegions`).
- Binary payloads (litematic/schem/GLB/PNG/…) cross the boundary base64-encoded
  (`*_b64` methods); arrays/lists cross as JSON strings.
- The mesh progress callback is replaced by a polling `MeshJob`
  (start → `poll_progress` → `take_result`).
- Memory management is generated: no more `free_*` functions anywhere.

See `src/bridge/PORTING.md` for the binding rules and
`tools/bridge_coverage/exclusions.txt` for the audited old→new name map.

**Complete API documentation across all bindings.** Every public function on
the bridge surface (509 total) now carries a doc comment, propagated by the
generator into all seven languages; the 140 previously undocumented functions
(meshing config, simulation value/layout/ordering types, transforms,
definition regions, …) were documented from their implementations, including
defaults, units, and coordinate/rotation conventions.

**Editing-operation performance.**
- `set_block_from_string` now caches parsed block strings (properties + NBT)
  per schematic, and placed block entities Arc-share the cached NBT
  (copy-on-write). Repeatedly placing the same NBT-bearing block (e.g. filled
  chests) is ~41× faster (0.30 → 12.4 M blocks/s); property-bearing blocks
  (e.g. repeaters) are ~3.6× faster (5.7 → 20.6 M blocks/s).
- `copy_region` from a single-region source (the common case) now translates
  palette indices through a precomputed source→target map instead of hashing
  a `BlockState` per block: ~3.8× faster (64 → 242 M blocks/s), same
  resulting content (covered by a fast-vs-slow-path equivalence test).

---

# Nucleation v0.2.18

Maintenance release, no user-facing API changes. The FFI layer
(`src/ffi.rs`, 10k+ lines) is now split into per-domain modules under
`src/ffi/`, matching the existing WASM/Python binding structure —
verified byte-identical exported C symbols across every feature
combination before and after. Format parsing (`src/formats/`,
`src/dataconverter/`) converged onto a proper `thiserror`-based error
type instead of ad-hoc `Box<dyn Error>`/`String` errors; the public
`UniversalSchematic::to_schematic`/`from_schematic` signatures are
unchanged. Also merged in a diff palette-swap-dominance feature that
had been sitting on an unmerged branch, cleared out several stale
branches, ran a full `clippy --fix` pass, and fixed a comparator
custom-IO test that had the wrong block orientation baked in (it now
actually exercises redpiler's IN→wire→OUT signal path instead of
silently testing nothing while ignored).

# Nucleation v0.2.17

JVM: packed bulk block export. Adds a palette + stride-4 int array
encoding for pulling large regions out of the JVM binding in one call,
instead of one JNI round-trip per block.

# Nucleation v0.2.16

SDF (signed distance field) shape and terrain generation, available
across every binding: build a schematic by sampling an SDF JSON tree
against material rules (`from_sdf` / `from_sdf_bounded`, with a
standalone `sdf_eval` for point queries). JVM also picked up
`setBlockEntity` with SNBT write support, and `from_insign` now strips
sign blocks after compiling their annotations instead of leaving them
in the output.

# Nucleation v0.2.15

Small follow-up to v0.2.14: `schematic-mesher` resolves from its
GitHub source again (a crates.io publish had briefly broken that),
plus full binding parity for the datafixer and entity/block-entity
SNBT API introduced in v0.2.14, and a `MeshOutput::from` fix so the
local (non-service) mesher path constructs by value correctly.

# Nucleation v0.2.14

The big one in this range. Relicensed the project from AGPL-3.0-only
to MIT across every file. Landed the streaming world API — constant-
memory parsing, generation, and diffing of world saves without holding
the whole world in memory, plus `.mca`/world-folder docs to match.
Added redstone graph export with integration tests, meshing
performance work (palette-indexed block sources), and merged in a
contributor's fork carrying dataconverter and litematic/entity
improvements.

Also where the JVM binding caught up hard on this window: full
`MchprsWorld` simulation parity with Python, the item-model generation
API, the redstone graph + typed circuit executor API, and a fix for
released fat JARs that had been shipping without `mchprs` compiled in
(simulation now on by default).

# Nucleation v0.2.13

Exposes `footprint()` — a translation-invariant shape fingerprint used
by the fingerprint/classification engine — as a vector across all
bindings, rather than only being reachable through the Rust API.

# Nucleation v0.2.12

New fingerprint & signature engine: canonical `Fingerprint`/`Signature`
types, symmetry-group-aware rigid transforms, an FFT-based
translation-invariant `Footprint`, a rule-based classifier with
shipped rulesets (structural, redstone computational/survival) loaded
from RON, and synthetic-fixture benchmarks. Exposed to WASM as `Diff`
and `Fingerprint` bindings. Also added synchronous Redis and S3 `Store`
backends alongside the existing filesystem one.

# Nucleation v0.2.11

Render background color and orthographic/isometric projection support
for `RenderConfig`, implemented in core and exposed across
Python/WASM/FFI (Python via a `Projection` enum, WASM/FFI via
`orthographic`/`setOrthographic`-style booleans — a documented,
intentional naming divergence, see `api_parity_exclusions.txt`). Also
fixes an `i64` overflow in `Region::coords_to_index` for large
regions.

# Nucleation v0.2.10

Build script fix. v0.2.9's `assemble-jvm-jar` job failed at the
`processResources` step under Gradle 9:

    Entry native/linux-arm64/libnucleation_jvm.so is a duplicate but no
    duplicate handling strategy has been set.

Two compounding sources of the duplicate:

1. `collectNatives` was copying `src/main/resources/native/**/*.{so,
   dylib,dll}` into `build/native-staging/`. Those files were already
   on the default resources classpath, so they got bundled twice.
2. `processResources` had no `duplicatesStrategy` set, which under
   Gradle 9 (strict by default) fails the build instead of warning.

Fixed in `nucleation-jvm/jvm/build.gradle.kts`:
- Dropped the redundant `preStaged` from() in `collectNatives` — pre-
  staged cdylibs reach the JAR through the default resources path
  alone, no need to re-copy them.
- Added `duplicatesStrategy = DuplicatesStrategy.EXCLUDE` to
  `processResources` as a safety net in case the host cargo target and
  a pre-staged cdylib happen to overlap on the same platform.

No source / API changes since v0.2.7.

v0.2.8 retired (deprecated macos-13 runner).
v0.2.9 retired (Gradle 9 duplicate-resources failure).

See v0.2.7 release notes for the feature work.
