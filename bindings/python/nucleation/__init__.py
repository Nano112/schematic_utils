"""nucleation — generated wire core + hand-written idiomatic veneer.

Two layers, by design:

* ``nucleation.core`` — the compiled Diplomat/nanobind extension, installed
  as the ``nucleation.nucleation`` submodule. This is the WIRE format:
  JSON-string arguments, positional splats, exact bridge shapes. It is
  regenerated; never edit it, never make users type it.
* this package — re-exports the whole core surface verbatim, then overlays
  the thin veneer in ``design.py`` (keyword arguments, tuples, dataclasses,
  typed reports; zero logic beyond marshalling). The JS package mirrors the
  same two layers 1:1.

Hand-written additions live here next to ``custom/`` (the C++ counterpart);
the generated sources stay untouched under ``src/``.
"""

from .nucleation import *  # noqa: F401,F403 — the generated core, verbatim
from . import nucleation as core  # noqa: F401 — explicit escape hatch

if hasattr(core, "Design"):
    from .design import (  # noqa: F401 — veneer overlays shadow core names
        Bus,
        CheckReport,
        Design,
        DesignCheckError,
        Executor,
        Flat,
        Gate,
        Style,
    )

from .curation import (  # noqa: F401 — pure-Python corpus curation layer
    CuratedCorpus,
    CurationDecision,
    CurationPolicy,
    MetricRule,
    curate_corpus,
    write_registry_archives,
    write_top_owner_archives,
)
from .processing import (  # noqa: F401 — typed facade over the shared JSON policy contract
    ContentPolicy,
    DecodeLimits,
    RegistryHookRule,
    RegistryPipelineConfig,
    MaterialProfile,
    TransformPlan,
    TransformReport,
    UuidPolicy,
    apply_transform,
    decode_bounded,
    inspect_transform,
)
