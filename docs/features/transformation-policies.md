# Transformation, normalization, and content policies

Nucleation processes untrusted or non-standard schematics through an explicit,
versioned transform plan:

```text
bounded decode -> inspect -> policy decision -> transform -> validate -> store
```

A plan is separate from serialization. It is applied to a clone and committed
only if no rule rejects it, so rejection leaves the source unchanged. Inspection
executes the same plan on a clone and returns the proposed findings without
committing it.

## Choose the smallest policy that fits

| Goal | Plan | Changes content? | Deterministic? |
|---|---|---:|---:|
| Stable palette/order only | `TransformPlan.canonical()` | No | Yes |
| Convert one building convention to another | `remap_materials` with a named profile | Yes | Yes |
| Screen an imported build for a public registry | `TransformPlan.registry_safe()` | Yes | Yes |
| Enforce project-specific content or identity rules | Custom ordered passes | Depends on actions | Yes, except random UUID regeneration |

Loading and saving never normalize implicitly. A caller must choose a plan,
which makes policy changes reviewable and prevents a format conversion from
silently changing authored content.

## Python

```python
from nucleation import Schematic
from nucleation import TransformPlan, apply_transform, inspect_transform

schematic = Schematic.open("untrusted.schem")
plan = TransformPlan.registry_safe()

preview = inspect_transform(schematic, plan)
if not preview.rejected and not preview.quarantined:
    report = apply_transform(schematic, plan)
    schematic.save("accepted.schem")
```

Decode untrusted input with explicit limits before applying a plan:

```python
from nucleation import DecodeLimits, decode_bounded

schematic = decode_bounded(payload, DecodeLimits(
    max_input_bytes=64 * 1024 * 1024,
    max_decompressed_bytes=256 * 1024 * 1024,
    max_volume=64_000_000,
))
```

The compiled cross-language surface accepts the same versioned JSON contract:

```python
report_json = schematic.inspect_transform_plan_json(plan.to_json())
report_json = schematic.apply_transform_plan_json(plan.to_json())
```

Policy rejection is represented by `report.rejected`; malformed plan JSON is an
API error. Reports use stable codes, paths, actions, and counts and never include
the removed or redacted value.

`TransformReport` contains `schema_version`, `plan`, `dry_run`, `rejected`,
`quarantined`, a stable code-to-count `summary`, and ordered `findings`. Each
finding contains `code`, `severity`, `action`, a schematic `path`, and an
optional non-sensitive `rule` label. Route on the booleans or stable codes,
never on prose.

The bridge surface is intentionally small and identical in every generated
language binding (with the normal casing convention):

| Operation | Purpose |
|---|---|
| `inspect_transform_plan_json(plan_json)` | Execute the exact plan on a clone and return report JSON |
| `apply_transform_plan_json(plan_json)` | Apply atomically and return report JSON, including policy rejection |
| `canonicalize_json()` | Apply the built-in lossless canonical plan |
| `inspect_registry_safe_json()` | Preview the built-in registry policy |
| `from_data_bounded(bytes, limits_json)` | Decode untrusted bytes with allocation limits |

The Python facade wraps these with typed dataclasses and `inspect_transform`,
`apply_transform`, and `decode_bounded`.

## Shared JSON contract

Plans use `schema_version: 1`, a non-empty stable name, a history switch, and an
ordered pass list. This complete example is valid in Rust and every binding:

```json
{
  "schema_version": 1,
  "name": "community-import-v1",
  "record_history": true,
  "passes": [
    { "type": "canonicalize_palette" },
    {
      "type": "remap_materials",
      "profile": {
        "name": "concrete-standard-v1",
        "version": 1,
        "mappings": { "minecraft:stone": "minecraft:gray_concrete" },
        "family_mappings": [],
        "preserve_unmentioned_properties": false,
        "safety": "profile"
      }
    },
    {
      "type": "content_policy",
      "policy": {
        "allowed_namespaces": ["minecraft"],
        "namespace_action": "quarantine",
        "text": {
          "strip_keys": ["CustomName"],
          "suspicious_patterns": ["ignore previous instructions"],
          "suspicious_action": "warn",
          "max_string_bytes": 32768,
          "oversize_action": "reject"
        },
        "entities": { "max_total": 512, "excess_action": "quarantine" },
        "uuids": {
          "mode": "regenerate_deterministic",
          "representation": "int_array",
          "salt": "example:community:v1"
        }
      }
    }
  ]
}
```

Omitted policy sections and fields receive their documented defaults. Always
bump the plan name when changing its meaning so audit history and downstream
registries can distinguish revisions.

## Plan passes

Version 1 includes three core passes:

- `canonicalize_palette`: losslessly sort properties and palette entries,
  deduplicate equivalent states, remove unused entries, and reserve palette
  index zero for air.
- `remap_materials`: apply a named, versioned exact or color-family material
  profile.
- `content_policy`: recursively inspect and transform entities, block entities,
  nested items, text, NBT, identifiers, and UUID references.

Passes run in listed order.

## Actions and policy field reference

Actions are shared by all applicable policy sections:

| Action | Result |
|---|---|
| `allow` | Keep content and emit no finding |
| `warn` | Keep content and add a warning finding |
| `redact` | Replace matching text with the configured redaction string |
| `remove` | Remove the matched value/entity/item, or replace a denied block with air |
| `quarantine` | Keep processing, mark the report for quarantine |
| `reject` | Mark the plan rejected; atomic apply leaves the source unchanged |

`ContentPolicy` contains the following sections. Empty allowlists mean “not
configured”; empty denylists match nothing.

| Section | Fields | Defaults and notes |
|---|---|---|
| Root | `allowed_namespaces`, `namespace_action` | All namespaces; `warn` |
| `text` | `strip_keys`, `redact_words`, `redaction`, `suspicious_patterns`, `suspicious_action`, `max_string_bytes`, `oversize_action` | Empty rules; replacement `[redacted]`; actions `warn`; no size cap |
| `nbt` | `max_depth`, `max_nodes`, `max_collection_items`, `limit_action`, `remove_keys`, `executable_keys`/`_action`, `profile_keys`/`_action`, `volatile_keys`/`_action` | Depth 64; other limits unset; actions `warn`. Aggregate limits cannot use `remove`/`redact` |
| `items` | `allowed_ids`, `denied_ids`, `denied_action`, `clear_inventories`, `max_inventory_items`, `excess_action` | No ID rules or cap; inventories retained |
| `blocks` | `allowed_ids`, `denied_ids`, `denied_action` | No ID rules; `warn` |
| `entities` | `allowed_ids`, `denied_ids`, `denied_action`, `max_total`, `max_per_region`, `max_per_1000_blocks`, `excess_action`, `remove_players` | No ID rules or caps; actions `warn`; players retained |
| `block_entities` | `allowed_ids`, `denied_ids`, `denied_action`, `max_total`, `max_per_region`, `max_per_1000_blocks`, `excess_action` | No ID rules or caps; actions `warn` |
| `uuids` | `mode`, `representation`, `scope`, `salt`, `identity_basis`, `assign_missing`, `collision`, `dangling`, `definition_keys`, `reference_keys` | Preserve values/shape; definitions and references; stable path; warn on collisions/dangling references |

Allow/deny ID rules are exact namespaced IDs. Namespace rules apply recursively
to recognized blocks, entities, block entities, and items. Text and NBT rules
also traverse nested item stacks and passengers rather than inspecting only the
top level.

## Transformation history

Successful plans append a content-addressed record to
`metadata.transformation_history`. The record contains the plan name and hash,
plan-schema version,
lossless/quarantine flags, the non-sensitive summary, and a `verification` map.
The core records plan validation, policy acceptance, and an actual transform-
twice idempotence check (`passed`, `failed`, or `not_applicable`). The check compares
serialized Sponge v3 bytes before and after reapplying the passes, with
`verification.idempotence_format = "sponge_v3"`. It excludes the record being
created and does not certify other formats or format-conversion round trips.
Compound keys are sorted recursively at export; list order is preserved.
The record deliberately has
no wall-clock timestamp, keeping deterministic output reproducible. Immediate
reapplication of the same plan is deduplicated. Source provenance is never
rewritten to record processing history.

Sponge `.schem`, Litematic, and Nucleation snapshot v3 persist the history.
Snapshot versions 1 and 2 remain readable.

## UUID policy

UUID policy is intentionally more detailed than a `strip_uuids` flag.

| Field | Choices and behavior |
|---|---|
| `mode` | `preserve`, `remove`, `regenerate_random`, or `regenerate_deterministic` |
| `representation` | Preserve the source shape or standardize to `int_array`, `string`, or `long_pair` |
| `scope` | Rewrite definitions only, definitions with references, or all recognized UUID-like keys |
| `salt` | Namespace for deterministic identities |
| `identity_basis` | `stable_path`, or `entity_location` for identities stable across entity reordering |
| `assign_missing` | Assign identities to top-level entities and nested passengers; requires a regeneration mode |
| `collision` | `warn`, `reject`, or knowingly `keep` duplicate definitions |
| `dangling` | `warn`, `remove`, `reject`, or `preserve` references without an in-schematic definition |
| `definition_keys` | Configure recognized UUID-definition field names |
| `reference_keys` | Configure owner, leash, trust, and other reference field names |

Deterministic identities derive from the selected stable identity and policy
salt, not repeatedly from the previous UUID. `stable_path` is canonical and
compact. `entity_location` uses region, entity type, and exact IEEE-754
position bits, so reordering entities does not change identities. Two entities
of the same type at the same exact position are surfaced through the configured
collision policy. The operation is therefore idempotent.
References are rewritten through the definition map so owners, passengers, and
other internal relationships keep the same identity. This includes direct
fields, nested owner/leash compounds, and UUID lists such as `Trusted`.
Unknown references are handled by `dangling`; they are never independently
randomized. A long-pair cannot be represented as one NBT-list element, so that
combination is reported and standardized to an int array for the list element.
The configurable definition and reference key sets let modded schemas opt in
without making every UUID-looking string an identity automatically.

```python
from nucleation import ContentPolicy, TransformPlan, UuidPolicy, apply_transform

uuid_policy = UuidPolicy(
    mode="regenerate_deterministic",
    representation="int_array",
    salt="schematio:ore:v1",
    identity_basis="entity_location",
    assign_missing=True,
    collision="reject",
    dangling="remove",
)

plan = TransformPlan.from_passes("ore-registry-v1", [{
    "type": "content_policy",
    "policy": ContentPolicy(uuids=uuid_policy).as_dict(),
}])
report = apply_transform(schematic, plan)
```

An `Owner` in trusted Nucleation extraction provenance is not Minecraft entity
NBT and is not traversed by these rules. Registries must still distinguish
verified provenance from arbitrary imported provenance before attribution.

## Text, NBT, items, and entities

Text rules can remove selected fields, redact configured words, warn or redact
on suspicious patterns, and enforce string-size limits. Pattern detection is a
policy signal rather than proof of malicious intent; warning or quarantine is
recommended for prompt-injection and code-like heuristics.

NBT policies limit depth, node counts, collection sizes, and independently
handle ordinary keys, executable/command fields, player/profile data, and
volatile state such as scheduled ticks. Item rules can clear inventories,
allow/deny item IDs, and cap inventory list sizes.
Block, entity, and block-entity policies support allowlists, denylists,
namespace rules, and `allow`, `warn`, `remove`, `reject`, or `quarantine`
decisions. Removing a block replaces it with air and also removes a block
entity orphaned at that coordinate. Entity policies additionally provide count
budgets globally, per region, and per 1,000 blocks, plus player exclusion.
Mobile entities and block entities remain
separate because item frames, minecarts, display entities, signs, barrels, and
command blocks have different preservation requirements.

The in-memory NBT budgets in a transform protect registry policy and output
shape. Bounded readers separately enforce input and decompressed bytes,
dimensions and checked volume, region/palette/entity counts, NBT depth, string
lengths, collection lengths, and total nodes before large allocations. Sponge,
Litematic, classic schematic, mcstructure, structure SNBT, and snapshot formats
implement this contract. Whole MCA and world-ZIP containers are deliberately
refused by the generic bounded reader; use the world-segment streaming API with
explicit world bounds so a complete world is never materialized implicitly.

`DecodeLimits` fields are `max_input_bytes`, `max_decompressed_bytes`,
`max_dimension`, `max_volume`, `max_regions`, `max_palette_entries`,
`max_entities`, `max_block_entities`, `max_nbt_depth`,
`max_nbt_string_bytes`, `max_nbt_collection_items`, and `max_nbt_nodes`. All
must be positive. Defaults are deliberately generous library ceilings; public
services should lower them to the largest object they intentionally accept.

## Material standards

Material profiles support exact mappings and family templates:

```python
from nucleation import MaterialProfile, TransformPlan

profile = MaterialProfile(
    name="concrete-standard-v1",
    safety="profile",
    mappings={"minecraft:stone": "minecraft:white_concrete"},
    family_mappings=[{
        "source": "minecraft:{color}_wool",
        "target": "minecraft:{color}_concrete",
    }],
)

plan = TransformPlan.from_passes("concrete-standard-v1", [{
    "type": "remap_materials",
    "profile": profile.as_dict(),
}])
```

| Profile field | Meaning |
|---|---|
| `name`, `version` | Stable profile identity; change it when mappings or semantics change |
| `target_data_version` | Optional Minecraft data version expected by the target convention |
| `mappings` | Exact source block state/ID to target block state mapping |
| `family_mappings` | `{color}` templates with optional explicit `values` (all dye colors by default) |
| `preserve_unmentioned_properties` | Copy source properties absent from the target state |
| `safety` | `exact`, `behavior_preserving`, `profile`, or `aggressive` |

Safety modes are `exact`, `behavior_preserving`, `profile`, and `aggressive`.
`exact` accepts only identical states. `behavior_preserving` classifies both
states using block data and conservative roles: kind, full-cube/collision,
transparency, emitted light, block-entity behavior, support needs, redstone
component status, piston class, and properties. It applies a mapping only when
those roles match. `profile` and `aggressive` apply explicitly requested convention maps
and report `material.behavior_not_proven`. Concrete, glass, slabs, movable
blocks, and support blocks are not interchangeable merely because they look
similar.

This role proof is intentionally conservative: unknown or modded blocks require
an explicit profile rather than being guessed compatible.

## Built-in presets and registry integration

- `canonical`: lossless deterministic palette normalization.
- `registry-safe-v1`: canonicalization, authored-text removal, suspicious-text
  warnings, removal of common ephemeral entities, an entity-count quarantine
  threshold, and deterministic UUID standardization.

`RegistryPipeline` is the first-class storage workflow. It streams a key from
any `Store` under `max_input_bytes`, bounded-decodes it, applies the plan
atomically, writes accepted/quarantined output as a deterministic Nucleation
snapshot, and persists a JSON audit report. Decode and policy failures route to
reject without writing a schematic.

`RegistryHookRule` is the scripting boundary. A rule examines only stable
summary counters and may escalate `accept` to `quarantine` or `reject`; it can
never override a rejection. Python and compiled extensions can emit this JSON
or consume the report out of process. Nucleation deliberately does not import
arbitrary callback code into the registry process.

The serializable pipeline configuration contains `decode_limits`, `plan`, the
four relative logical key prefixes (`accept_prefix`, `quarantine_prefix`,
`reject_prefix`, `report_prefix`), and `hooks`. Each hook has `summary_code`,
`minimum` (default 1), and a target `route`. An ingest report contains
`schema_version`, `source_key`, the input BLAKE3 `object_id`, selected `route`,
detected format, output/route/report keys, an optional stable `error_code`, and
the optional transform report.

```rust
use nucleation::{MemStore, RegistryPipeline, Store};

let input = MemStore::new();
let output = MemStore::new();
input.put("incoming/build.schem", &payload)?;
let result = RegistryPipeline::default()
    .ingest_store(&input, "incoming/build.schem", &output)?;
println!("{:?}: {}", result.route, result.report_key);
```

All policy, limit, hook, and report contracts remain serializable so Rust,
Python, JavaScript/WASM, Kotlin/JVM, PHP, and C/C++ share the same semantics.

Registry outputs are content-addressed by BLAKE3. Accepted and quarantined
schematics are deterministic `.nusn` snapshots; every attempt writes a JSON
report, while rejects additionally write their route record and never write a
schematic. Hooks read only stable summary counters and can only escalate the
route (`accept` -> `quarantine` -> `reject`).

## Operational checklist

1. Bounded-decode before inspecting untrusted input.
2. Give each policy revision a new stable plan/profile name and salt namespace.
3. Dry-run and retain the report before applying destructive actions.
4. Treat quarantine as requiring an explicit review decision.
5. Preserve source provenance; transformation history records processing and is
   intentionally separate.
6. Store the normalized object and its audit report together.
7. Test custom policies for rejection atomicity, determinism, idempotence, and
   format round trips before deploying them.

## Required conformance tests

Every built-in policy must cover:

- dry-run and apply equivalence;
- atomic rejection;
- deterministic and idempotent application where the mode permits it;
- nested items, passengers, profiles, owners, and block entities;
- UUID definitions, references, collisions, dangling references, and all
  supported representations;
- transform-twice and format round trips;
- stable reports that do not echo private values;
- malformed, deeply nested, oversized, unknown, and modded fixtures;
- property-style mutation tests and parser fuzzing for untrusted data.

The checked-in `fuzz/` package contains `bounded_decode` and `policy_json`
`cargo-fuzz` targets. `tests/policy_conformance.rs` supplies deterministic CI
coverage for the same panic-safety and non-leak invariants.
