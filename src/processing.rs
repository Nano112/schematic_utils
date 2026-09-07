//! Policy-driven, auditable schematic processing.
//!
//! A [`TransformPlan`] is an ordered, serializable list of passes. Plans are
//! applied atomically: the source schematic changes only after every pass has
//! completed without a rejecting policy finding. [`TransformPlan::inspect`]
//! runs the exact same code on a clone and returns the report without changing
//! the source.

use crate::entity::NbtValue as EntityNbt;
use crate::nbt::{NbtMap, NbtValue};
use crate::{BlockState, Region, UniversalSchematic};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

pub const TRANSFORM_POLICY_SCHEMA_VERSION: u32 = 1;
pub const TRANSFORM_REPORT_SCHEMA_VERSION: u32 = 1;

fn schema_version() -> u32 {
    TRANSFORM_POLICY_SCHEMA_VERSION
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[default]
    Allow,
    Warn,
    Redact,
    Remove,
    Reject,
    Quarantine,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    #[default]
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformFinding {
    pub code: String,
    pub severity: Severity,
    pub action: Action,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformReport {
    pub schema_version: u32,
    pub plan: String,
    pub dry_run: bool,
    pub rejected: bool,
    pub quarantined: bool,
    pub summary: BTreeMap<String, u64>,
    pub findings: Vec<TransformFinding>,
}

impl TransformReport {
    fn new(plan: String, dry_run: bool) -> Self {
        Self {
            schema_version: TRANSFORM_REPORT_SCHEMA_VERSION,
            plan,
            dry_run,
            rejected: false,
            quarantined: false,
            summary: BTreeMap::new(),
            findings: Vec::new(),
        }
    }

    fn count(&mut self, code: &str, amount: usize) {
        if amount > 0 {
            *self.summary.entry(code.to_string()).or_default() += amount as u64;
        }
    }

    fn finding(&mut self, code: &str, action: Action, path: impl Into<String>, rule: Option<&str>) {
        if action == Action::Allow {
            return;
        }
        self.rejected |= action == Action::Reject;
        self.quarantined |= action == Action::Quarantine;
        let severity = match action {
            Action::Reject => Severity::Error,
            Action::Warn | Action::Quarantine => Severity::Warning,
            _ => Severity::Info,
        };
        self.count(code, 1);
        self.findings.push(TransformFinding {
            code: code.to_string(),
            severity,
            action,
            path: path.into(),
            rule: rule.map(str::to_string),
        });
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransformError {
    InvalidPlan(String),
    Rejected(TransformReport),
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlan(message) => write!(f, "invalid transform plan: {message}"),
            Self::Rejected(_) => write!(f, "transform rejected by policy"),
        }
    }
}

impl std::error::Error for TransformError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformPlan {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default = "default_true")]
    pub record_history: bool,
    #[serde(default)]
    pub passes: Vec<TransformSpec>,
}

fn default_true() -> bool {
    true
}

impl TransformPlan {
    pub fn new(name: impl Into<String>, passes: Vec<TransformSpec>) -> Self {
        Self {
            schema_version: TRANSFORM_POLICY_SCHEMA_VERSION,
            name: name.into(),
            record_history: true,
            passes,
        }
    }

    pub fn from_json(json: &str) -> Result<Self, TransformError> {
        let plan: Self = serde_json::from_str(json)
            .map_err(|error| TransformError::InvalidPlan(error.to_string()))?;
        plan.validate()?;
        Ok(plan)
    }

    pub fn to_json(&self) -> Result<String, TransformError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| TransformError::InvalidPlan(error.to_string()))
    }

    pub fn validate(&self) -> Result<(), TransformError> {
        if self.schema_version != TRANSFORM_POLICY_SCHEMA_VERSION {
            return Err(TransformError::InvalidPlan(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        if self.name.trim().is_empty() {
            return Err(TransformError::InvalidPlan(
                "plan name must not be empty".to_string(),
            ));
        }
        for pass in &self.passes {
            pass.validate()?;
        }
        Ok(())
    }

    /// Apply the complete plan atomically. A rejection returns its audit
    /// report and leaves `schematic` byte-for-byte unchanged in memory.
    pub fn apply(
        &self,
        schematic: &mut UniversalSchematic,
    ) -> Result<TransformReport, TransformError> {
        self.validate()?;
        let mut candidate = schematic.clone();
        let report = self.run(&mut candidate, false)?;
        if report.rejected {
            return Err(TransformError::Rejected(report));
        }
        *schematic = candidate;
        Ok(report)
    }

    /// Run the same plan without modifying `schematic`.
    pub fn inspect(
        &self,
        schematic: &UniversalSchematic,
    ) -> Result<TransformReport, TransformError> {
        self.validate()?;
        let mut candidate = schematic.clone();
        self.run(&mut candidate, true)
    }

    fn run(
        &self,
        schematic: &mut UniversalSchematic,
        dry_run: bool,
    ) -> Result<TransformReport, TransformError> {
        let mut report = TransformReport::new(self.name.clone(), dry_run);
        for pass in &self.passes {
            match pass {
                TransformSpec::CanonicalizePalette => {
                    for_each_region_mut(schematic, |path, region| {
                        let (before, after, rewritten) = region.canonicalize_palette();
                        report.count("palette.entries_removed", before.saturating_sub(after));
                        report.count("palette.cells_reindexed", rewritten);
                        if before != after || rewritten > 0 {
                            report.finding("palette.canonicalized", Action::Allow, path, None);
                        }
                    });
                }
                TransformSpec::RemapMaterials { profile } => {
                    apply_material_profile(schematic, profile, &mut report);
                }
                TransformSpec::ContentPolicy { policy } => {
                    apply_content_policy(schematic, policy, &mut report)?;
                }
            }
        }
        if !dry_run && !report.rejected && self.record_history {
            let plan_id = self.content_id()?;
            let already_recorded = schematic
                .metadata
                .transformation_history
                .last()
                .is_some_and(|record| record.plan_id == plan_id);
            if !already_recorded {
                let mut verification = BTreeMap::from([
                    ("plan_validated".to_string(), "passed".to_string()),
                    ("policy_accepted".to_string(), "passed".to_string()),
                ]);
                if self.is_deterministic() {
                    // Compare exported artifacts, not in-memory map equality.
                    // History is appended below so verification cannot include
                    // itself. Format conversion/reader normalization is separate.
                    let before = crate::formats::schematic::to_schematic(schematic)
                        .map_err(|error| TransformError::InvalidPlan(error.to_string()))?;
                    let mut probe = schematic.clone();
                    let probe_report = self.run(&mut probe, true)?;
                    let after = crate::formats::schematic::to_schematic(&probe)
                        .map_err(|error| TransformError::InvalidPlan(error.to_string()))?;
                    verification.insert("idempotence_format".to_string(), "sponge_v3".to_string());
                    verification.insert(
                        "idempotence".to_string(),
                        if !probe_report.rejected && before == after {
                            "passed"
                        } else {
                            "failed"
                        }
                        .to_string(),
                    );
                } else {
                    verification.insert("idempotence".to_string(), "not_applicable".to_string());
                }
                let lossless = !report.findings.iter().any(|finding| {
                    matches!(finding.action, Action::Redact | Action::Remove)
                }) && !self.passes.iter().any(|pass| {
                    matches!(
                        pass,
                        TransformSpec::RemapMaterials { profile }
                            if matches!(profile.safety, MaterialSafety::Profile | MaterialSafety::Aggressive)
                    )
                });
                schematic.metadata.transformation_history.push(
                    crate::metadata::TransformationRecord {
                        schema_version: 1,
                        plan_schema_version: self.schema_version,
                        plan_name: self.name.clone(),
                        plan_id,
                        lossless,
                        quarantined: report.quarantined,
                        summary: report.summary.clone(),
                        verification,
                    },
                );
            }
        }
        report.findings.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.code.cmp(&b.code))
                .then(a.rule.cmp(&b.rule))
        });
        Ok(report)
    }

    pub fn content_id(&self) -> Result<String, TransformError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| TransformError::InvalidPlan(error.to_string()))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    /// Whether applying this plan to identical input is expected to produce
    /// identical output. Random UUID regeneration is the sole nondeterministic
    /// transform in the v1 policy contract.
    pub fn is_deterministic(&self) -> bool {
        !self.passes.iter().any(|pass| {
            matches!(
                pass,
                TransformSpec::ContentPolicy { policy }
                    if policy.uuids.mode == UuidMode::RegenerateRandom
            )
        })
    }

    pub fn canonical() -> Self {
        Self::new("canonical", vec![TransformSpec::CanonicalizePalette])
    }

    pub fn registry_safe() -> Self {
        Self::new(
            "registry-safe-v1",
            vec![
                TransformSpec::CanonicalizePalette,
                TransformSpec::ContentPolicy {
                    policy: ContentPolicy::registry_safe(),
                },
            ],
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformSpec {
    CanonicalizePalette,
    RemapMaterials { profile: MaterialProfile },
    ContentPolicy { policy: ContentPolicy },
}

impl TransformSpec {
    fn validate(&self) -> Result<(), TransformError> {
        match self {
            Self::CanonicalizePalette => Ok(()),
            Self::RemapMaterials { profile } => profile.validate(),
            Self::ContentPolicy { policy } => policy.validate(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialProfile {
    pub name: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub target_data_version: Option<i32>,
    /// Source block string or block ID -> target block string.
    #[serde(default)]
    pub mappings: BTreeMap<String, String>,
    /// Template mappings such as `minecraft:{color}_wool` to
    /// `minecraft:{color}_concrete`.
    #[serde(default)]
    pub family_mappings: Vec<MaterialFamilyRule>,
    #[serde(default)]
    pub preserve_unmentioned_properties: bool,
    #[serde(default)]
    pub safety: MaterialSafety,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialFamilyRule {
    pub source: String,
    pub target: String,
    #[serde(default = "default_dye_colors")]
    pub values: Vec<String>,
}

fn default_dye_colors() -> Vec<String> {
    [
        "white",
        "orange",
        "magenta",
        "light_blue",
        "yellow",
        "lime",
        "pink",
        "gray",
        "light_gray",
        "cyan",
        "purple",
        "blue",
        "brown",
        "green",
        "red",
        "black",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialSafety {
    Exact,
    #[default]
    BehaviorPreserving,
    Profile,
    Aggressive,
}

/// Conservative behavioral facts used to decide whether a material mapping
/// can run without an explicit lossy-profile opt in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRole {
    pub known: bool,
    pub kind: String,
    pub full_cube: bool,
    pub transparent: bool,
    pub emits_light: bool,
    pub has_block_entity: bool,
    pub needs_support: bool,
    pub redstone_component: bool,
    pub piston_class: String,
}

impl BlockRole {
    pub fn classify(block: &BlockState) -> Self {
        let facts = crate::blockpedia::get_block(&block.name);
        let id = block.name.as_str();
        let needs_support = needs_support_conservatively(id);
        #[cfg(feature = "mc-tick")]
        let redstone_component = mc_tick::vanilla::is_simulation_component(&block.to_string());
        #[cfg(not(feature = "mc-tick"))]
        let redstone_component = is_redstone_name(id);
        let piston_class = if id.ends_with(":obsidian")
            || id.ends_with(":crying_obsidian")
            || id.ends_with(":reinforced_deepslate")
            || id.ends_with(":moving_piston")
            || id.ends_with(":piston_head")
        {
            "immovable"
        } else if id.ends_with(":slime_block") {
            "slime"
        } else if id.ends_with(":honey_block") {
            "honey"
        } else {
            "ordinary"
        };
        Self {
            known: facts.is_some(),
            kind: facts.map_or_else(|| "unknown".into(), |value| value.kind().into()),
            full_cube: facts.is_some_and(|value| value.is_full_cube()),
            transparent: facts.is_none_or(|value| value.transparent),
            emits_light: facts.is_some_and(|value| value.is_light_source()),
            has_block_entity: facts.is_some_and(|value| value.has_block_entity()),
            needs_support,
            redstone_component,
            piston_class: piston_class.into(),
        }
    }

    pub fn behavior_equivalent(source: &BlockState, target: &BlockState) -> bool {
        let source_role = Self::classify(source);
        let target_role = Self::classify(target);
        source_role.known
            && target_role.known
            && source_role == target_role
            && source.properties == target.properties
    }
}

fn needs_support_conservatively(id: &str) -> bool {
    [
        "torch",
        "flower",
        "fern",
        "sapling",
        "mushroom",
        "wheat",
        "carrot",
        "potato",
        "beetroot",
        "sugar_cane",
        "cactus",
        "bamboo",
        "vine",
        "lily_pad",
        "seagrass",
        "kelp",
        "coral",
        "button",
        "lever",
        "sign",
        "banner",
        "rail",
        "tripwire",
        "pressure_plate",
        "redstone_wire",
        "repeater",
        "comparator",
    ]
    .iter()
    .any(|part| id.contains(part))
}

#[cfg(not(feature = "mc-tick"))]
fn is_redstone_name(id: &str) -> bool {
    [
        "redstone",
        "repeater",
        "comparator",
        "piston",
        "observer",
        "lever",
        "button",
        "pressure_plate",
        "target",
        "tripwire",
        "dispenser",
        "dropper",
        "hopper",
        "crafter",
        "copper_bulb",
        "note_block",
        "jukebox",
        "barrel",
        "lectern",
    ]
    .iter()
    .any(|part| id.contains(part))
}

impl MaterialProfile {
    fn validate(&self) -> Result<(), TransformError> {
        if self.name.trim().is_empty() {
            return Err(TransformError::InvalidPlan(
                "material profile name must not be empty".into(),
            ));
        }
        for (source, target) in &self.mappings {
            BlockState::from_block_string(source).map_err(TransformError::InvalidPlan)?;
            BlockState::from_block_string(target).map_err(TransformError::InvalidPlan)?;
        }
        for rule in &self.family_mappings {
            if rule.source.matches("{color}").count() != 1
                || rule.target.matches("{color}").count() != 1
            {
                return Err(TransformError::InvalidPlan(
                    "material family source and target must each contain one {color} token".into(),
                ));
            }
            if rule.values.is_empty() {
                return Err(TransformError::InvalidPlan(
                    "material family values must not be empty".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentPolicy {
    #[serde(default)]
    pub allowed_namespaces: Option<BTreeSet<String>>,
    #[serde(default)]
    pub namespace_action: Action,
    #[serde(default)]
    pub text: TextPolicy,
    #[serde(default)]
    pub nbt: NbtPolicy,
    #[serde(default)]
    pub items: ItemPolicy,
    #[serde(default)]
    pub blocks: BlockPolicy,
    #[serde(default)]
    pub entities: EntityPolicy,
    #[serde(default)]
    pub block_entities: BlockEntityPolicy,
    #[serde(default)]
    pub uuids: UuidPolicy,
}

impl Default for ContentPolicy {
    fn default() -> Self {
        Self {
            allowed_namespaces: None,
            namespace_action: Action::Warn,
            text: TextPolicy::default(),
            nbt: NbtPolicy::default(),
            items: ItemPolicy::default(),
            blocks: BlockPolicy::default(),
            entities: EntityPolicy::default(),
            block_entities: BlockEntityPolicy::default(),
            uuids: UuidPolicy::default(),
        }
    }
}

impl ContentPolicy {
    pub fn registry_safe() -> Self {
        let mut strip_keys = BTreeSet::new();
        for key in ["CustomName", "pages", "filtered_pages", "author", "title"] {
            strip_keys.insert(key.to_string());
        }
        Self {
            text: TextPolicy {
                strip_keys,
                suspicious_patterns: vec![
                    "ignore previous instructions".into(),
                    "system prompt".into(),
                    "<script".into(),
                    "javascript:".into(),
                    "${jndi:".into(),
                ],
                suspicious_action: Action::Warn,
                ..TextPolicy::default()
            },
            nbt: NbtPolicy {
                executable_keys: [
                    "Command",
                    "command",
                    "LastOutput",
                    "UpdateLastExecution",
                    "auto",
                    "conditionMet",
                    "powered",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
                executable_action: Action::Remove,
                profile_keys: ["SkullOwner", "Profile", "profile"]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                profile_action: Action::Remove,
                volatile_keys: [
                    "Motion",
                    "FallDistance",
                    "Fire",
                    "Air",
                    "PortalCooldown",
                    "HurtTime",
                    "DeathTime",
                    "block_ticks",
                    "fluid_ticks",
                    "TileTicks",
                    "LiquidTicks",
                    "ScheduledTicks",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
                volatile_action: Action::Remove,
                ..NbtPolicy::default()
            },
            entities: EntityPolicy {
                denied_ids: [
                    "minecraft:item",
                    "minecraft:experience_orb",
                    "minecraft:area_effect_cloud",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
                denied_action: Action::Remove,
                max_total: Some(512),
                excess_action: Action::Quarantine,
                ..EntityPolicy::default()
            },
            uuids: UuidPolicy {
                mode: UuidMode::RegenerateDeterministic,
                representation: UuidRepresentation::IntArray,
                salt: "nucleation:registry-safe:v1".into(),
                dangling: DanglingReferencePolicy::Warn,
                ..UuidPolicy::default()
            },
            ..Self::default()
        }
    }

    fn validate(&self) -> Result<(), TransformError> {
        if self.text.redaction.is_empty() && !self.text.redact_words.is_empty() {
            return Err(TransformError::InvalidPlan(
                "text redaction replacement must not be empty".into(),
            ));
        }
        if self.nbt.max_depth == 0 {
            return Err(TransformError::InvalidPlan(
                "NBT max_depth must be greater than zero".into(),
            ));
        }
        if matches!(self.nbt.limit_action, Action::Remove | Action::Redact) {
            return Err(TransformError::InvalidPlan(
                "NBT aggregate limits support allow/warn/quarantine/reject; use field or item rules for removal"
                    .into(),
            ));
        }
        if self.uuids.assign_missing
            && !matches!(
                self.uuids.mode,
                UuidMode::RegenerateRandom | UuidMode::RegenerateDeterministic
            )
        {
            return Err(TransformError::InvalidPlan(
                "UUID assign_missing requires a regeneration mode".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPolicy {
    #[serde(default)]
    pub strip_keys: BTreeSet<String>,
    #[serde(default)]
    pub redact_words: Vec<String>,
    #[serde(default = "default_redaction")]
    pub redaction: String,
    #[serde(default)]
    pub suspicious_patterns: Vec<String>,
    #[serde(default)]
    pub suspicious_action: Action,
    #[serde(default)]
    pub max_string_bytes: Option<usize>,
    #[serde(default)]
    pub oversize_action: Action,
}

fn default_redaction() -> String {
    "[redacted]".to_string()
}

impl Default for TextPolicy {
    fn default() -> Self {
        Self {
            strip_keys: BTreeSet::new(),
            redact_words: Vec::new(),
            redaction: default_redaction(),
            suspicious_patterns: Vec::new(),
            suspicious_action: Action::Warn,
            max_string_bytes: None,
            oversize_action: Action::Warn,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NbtPolicy {
    #[serde(default = "default_nbt_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub max_nodes: Option<usize>,
    #[serde(default)]
    pub max_collection_items: Option<usize>,
    #[serde(default)]
    pub limit_action: Action,
    #[serde(default)]
    pub remove_keys: BTreeSet<String>,
    #[serde(default)]
    pub executable_keys: BTreeSet<String>,
    #[serde(default)]
    pub executable_action: Action,
    #[serde(default)]
    pub profile_keys: BTreeSet<String>,
    #[serde(default)]
    pub profile_action: Action,
    #[serde(default)]
    pub volatile_keys: BTreeSet<String>,
    #[serde(default)]
    pub volatile_action: Action,
}

fn default_nbt_depth() -> usize {
    64
}

impl Default for NbtPolicy {
    fn default() -> Self {
        Self {
            max_depth: default_nbt_depth(),
            max_nodes: None,
            max_collection_items: None,
            limit_action: Action::Warn,
            remove_keys: BTreeSet::new(),
            executable_keys: BTreeSet::new(),
            executable_action: Action::Warn,
            profile_keys: BTreeSet::new(),
            profile_action: Action::Warn,
            volatile_keys: BTreeSet::new(),
            volatile_action: Action::Warn,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPolicy {
    #[serde(default)]
    pub allowed_ids: Option<BTreeSet<String>>,
    #[serde(default)]
    pub denied_ids: BTreeSet<String>,
    #[serde(default)]
    pub denied_action: Action,
    #[serde(default)]
    pub clear_inventories: bool,
    #[serde(default)]
    pub max_inventory_items: Option<usize>,
    #[serde(default)]
    pub excess_action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPolicy {
    #[serde(default)]
    pub allowed_ids: Option<BTreeSet<String>>,
    #[serde(default)]
    pub denied_ids: BTreeSet<String>,
    #[serde(default)]
    pub denied_action: Action,
}

impl Default for BlockPolicy {
    fn default() -> Self {
        Self {
            allowed_ids: None,
            denied_ids: BTreeSet::new(),
            denied_action: Action::Warn,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPolicy {
    #[serde(default)]
    pub allowed_ids: Option<BTreeSet<String>>,
    #[serde(default)]
    pub denied_ids: BTreeSet<String>,
    #[serde(default)]
    pub denied_action: Action,
    #[serde(default)]
    pub max_total: Option<usize>,
    #[serde(default)]
    pub max_per_region: Option<usize>,
    #[serde(default)]
    pub max_per_1000_blocks: Option<usize>,
    #[serde(default)]
    pub excess_action: Action,
    #[serde(default)]
    pub remove_players: bool,
}

impl Default for EntityPolicy {
    fn default() -> Self {
        Self {
            allowed_ids: None,
            denied_ids: BTreeSet::new(),
            denied_action: Action::Warn,
            max_total: None,
            max_per_region: None,
            max_per_1000_blocks: None,
            excess_action: Action::Warn,
            remove_players: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockEntityPolicy {
    #[serde(default)]
    pub allowed_ids: Option<BTreeSet<String>>,
    #[serde(default)]
    pub denied_ids: BTreeSet<String>,
    #[serde(default)]
    pub denied_action: Action,
    #[serde(default)]
    pub max_total: Option<usize>,
    #[serde(default)]
    pub max_per_region: Option<usize>,
    #[serde(default)]
    pub max_per_1000_blocks: Option<usize>,
    #[serde(default)]
    pub excess_action: Action,
}

impl Default for BlockEntityPolicy {
    fn default() -> Self {
        Self {
            allowed_ids: None,
            denied_ids: BTreeSet::new(),
            denied_action: Action::Warn,
            max_total: None,
            max_per_region: None,
            max_per_1000_blocks: None,
            excess_action: Action::Warn,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UuidMode {
    #[default]
    Preserve,
    Remove,
    RegenerateRandom,
    RegenerateDeterministic,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UuidRepresentation {
    #[default]
    Preserve,
    IntArray,
    String,
    LongPair,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UuidScope {
    DefinitionsOnly,
    #[default]
    DefinitionsAndReferences,
    AllRecognized,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    #[default]
    Warn,
    Reject,
    Keep,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DanglingReferencePolicy {
    #[default]
    Warn,
    Remove,
    Reject,
    Preserve,
}

/// Stable identity used as the deterministic UUID namespace key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UuidIdentityBasis {
    /// Canonical region/entity/NBT path. Stable when ordering is stable.
    #[default]
    StablePath,
    /// Region, entity type, and exact position bits. Stable across entity
    /// reordering; duplicates are governed by the collision policy.
    EntityLocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UuidPolicy {
    #[serde(default)]
    pub mode: UuidMode,
    #[serde(default)]
    pub representation: UuidRepresentation,
    #[serde(default)]
    pub scope: UuidScope,
    #[serde(default)]
    pub salt: String,
    #[serde(default)]
    pub identity_basis: UuidIdentityBasis,
    #[serde(default)]
    pub assign_missing: bool,
    #[serde(default)]
    pub collision: CollisionPolicy,
    #[serde(default)]
    pub dangling: DanglingReferencePolicy,
    #[serde(default)]
    pub definition_keys: BTreeSet<String>,
    #[serde(default)]
    pub reference_keys: BTreeSet<String>,
}

impl Default for UuidPolicy {
    fn default() -> Self {
        Self {
            mode: UuidMode::Preserve,
            representation: UuidRepresentation::Preserve,
            scope: UuidScope::DefinitionsAndReferences,
            salt: String::new(),
            identity_basis: UuidIdentityBasis::StablePath,
            assign_missing: false,
            collision: CollisionPolicy::Warn,
            dangling: DanglingReferencePolicy::Warn,
            definition_keys: ["UUID", "uuid"].into_iter().map(str::to_string).collect(),
            reference_keys: [
                "Owner",
                "OwnerUUID",
                "Leash",
                "LoveCause",
                "ConversionPlayer",
                "HurtBy",
                "AngryAt",
                "Thrower",
                "Trusted",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}

fn namespace(id: &str) -> &str {
    id.split_once(':')
        .map(|(namespace, _)| namespace)
        .unwrap_or("minecraft")
}

fn for_each_region_mut(schematic: &mut UniversalSchematic, mut f: impl FnMut(String, &mut Region)) {
    let default_name = schematic.default_region_name.clone();
    f(
        format!("regions.{default_name}"),
        &mut schematic.default_region,
    );
    let mut names: Vec<String> = schematic.other_regions.keys().cloned().collect();
    names.sort();
    for name in names {
        if let Some(region) = schematic.other_regions.get_mut(&name) {
            f(format!("regions.{name}"), region);
        }
    }
}

fn apply_material_profile(
    schematic: &mut UniversalSchematic,
    profile: &MaterialProfile,
    report: &mut TransformReport,
) {
    for_each_region_mut(schematic, |path, region| {
        let mut skipped = 0usize;
        let changed = region.transform_palette_states(|source| {
            let full = source.to_string();
            let target = profile
                .mappings
                .get(&full)
                .or_else(|| profile.mappings.get(source.name.as_str()))
                .cloned()
                .or_else(|| material_family_target(source.name.as_str(), &profile.family_mappings));
            let Some(target) = target else {
                return source.clone();
            };
            let Ok(mut target) = BlockState::from_block_string(&target) else {
                return source.clone();
            };
            let verified = match profile.safety {
                MaterialSafety::Exact => target == *source,
                MaterialSafety::BehaviorPreserving => {
                    BlockRole::behavior_equivalent(source, &target)
                }
                MaterialSafety::Profile | MaterialSafety::Aggressive => false,
            };
            if matches!(
                profile.safety,
                MaterialSafety::Exact | MaterialSafety::BehaviorPreserving
            ) && !verified
            {
                skipped += 1;
                return source.clone();
            }
            if verified && target != *source {
                report.count("material.behavior_verified", 1);
            }
            if profile.preserve_unmentioned_properties && !target.properties.is_empty() {
                let explicit: BTreeSet<String> = target
                    .properties
                    .iter()
                    .map(|(key, _)| key.to_string())
                    .collect();
                for (key, value) in &source.properties {
                    if !explicit.contains(key.as_str()) {
                        target.properties.push((key.clone(), value.clone()));
                    }
                }
                target.properties.sort_by(|a, b| a.0.cmp(&b.0));
            } else if profile.preserve_unmentioned_properties && target.properties.is_empty() {
                target.properties = source.properties.clone();
            }
            target
        });
        report.count("material.cells_remapped", changed);
        report.count("material.palette_mappings_skipped", skipped);
        if skipped > 0 {
            report.finding(
                "material.behavior_equivalence_unproven",
                Action::Warn,
                &path,
                Some(&profile.name),
            );
        }
        if changed > 0
            && matches!(
                profile.safety,
                MaterialSafety::Profile | MaterialSafety::Aggressive
            )
        {
            report.finding(
                "material.behavior_not_proven",
                Action::Warn,
                path,
                Some(&profile.name),
            );
        }
    });
}

fn material_family_target(name: &str, rules: &[MaterialFamilyRule]) -> Option<String> {
    for rule in rules {
        for value in &rule.values {
            if rule.source.replace("{color}", value) == name {
                return Some(rule.target.replace("{color}", value));
            }
        }
    }
    None
}

fn apply_content_policy(
    schematic: &mut UniversalSchematic,
    policy: &ContentPolicy,
    report: &mut TransformReport,
) -> Result<(), TransformError> {
    let entity_total = total_entities(schematic);
    if policy
        .entities
        .max_total
        .is_some_and(|maximum| entity_total > maximum)
    {
        report.finding(
            "entity.total_limit_exceeded",
            policy.entities.excess_action,
            "entities",
            Some("max_total"),
        );
        if matches!(
            policy.entities.excess_action,
            Action::Remove | Action::Redact
        ) {
            trim_entities_to(schematic, policy.entities.max_total.unwrap(), report);
        }
    }
    let block_entity_total = total_block_entities(schematic);
    if policy
        .block_entities
        .max_total
        .is_some_and(|maximum| block_entity_total > maximum)
    {
        report.finding(
            "block_entity.total_limit_exceeded",
            policy.block_entities.excess_action,
            "block_entities",
            Some("max_total"),
        );
        if matches!(
            policy.block_entities.excess_action,
            Action::Remove | Action::Redact
        ) {
            trim_block_entities_to(schematic, policy.block_entities.max_total.unwrap(), report);
        }
    }

    for_each_region_mut(schematic, |region_path, region| {
        enforce_region_density_budgets(region, &region_path, policy, report);
    });

    for_each_region_mut(schematic, |region_path, region| {
        process_blocks(region, &region_path, policy, report);
    });

    // Structural removals happen before path-based UUID assignment. This keeps
    // deterministic UUIDs stable across repeated application even when an
    // earlier entity in the same region is removed by the policy.
    for_each_region_mut(schematic, |region_path, region| {
        prefilter_removed_entities(region, &region_path, policy, report);
        prefilter_removed_block_entities(region, &region_path, policy, report);
    });
    assign_missing_uuids(schematic, &policy.uuids, report);
    let uuid_map = build_uuid_map(schematic, &policy.uuids, report);
    for_each_region_mut(schematic, |region_path, region| {
        process_entities(region, &region_path, policy, &uuid_map, report);
        process_block_entities(region, &region_path, policy, &uuid_map, report);
    });
    Ok(())
}

fn enforce_region_density_budgets(
    region: &mut Region,
    region_path: &str,
    policy: &ContentPolicy,
    report: &mut TransformReport,
) {
    let block_units = region.count_blocks().max(1).div_ceil(1000);
    let entity_limit = [
        policy.entities.max_per_region,
        policy
            .entities
            .max_per_1000_blocks
            .and_then(|limit| limit.checked_mul(block_units)),
    ]
    .into_iter()
    .flatten()
    .min();
    if entity_limit.is_some_and(|limit| region.entities.len() > limit) {
        report.finding(
            "entity.region_density_exceeded",
            policy.entities.excess_action,
            format!("{region_path}.entities"),
            Some("region_budget"),
        );
        if matches!(
            policy.entities.excess_action,
            Action::Remove | Action::Redact
        ) {
            let limit = entity_limit.unwrap();
            report.count("entity.excess_removed", region.entities.len() - limit);
            region.entities.truncate(limit);
        }
    }

    let block_entity_limit = [
        policy.block_entities.max_per_region,
        policy
            .block_entities
            .max_per_1000_blocks
            .and_then(|limit| limit.checked_mul(block_units)),
    ]
    .into_iter()
    .flatten()
    .min();
    if block_entity_limit.is_some_and(|limit| region.block_entities.len() > limit) {
        report.finding(
            "block_entity.region_density_exceeded",
            policy.block_entities.excess_action,
            format!("{region_path}.block_entities"),
            Some("region_budget"),
        );
        if matches!(
            policy.block_entities.excess_action,
            Action::Remove | Action::Redact
        ) {
            let limit = block_entity_limit.unwrap();
            let mut entries = region.block_entities.drain();
            entries.sort_by_key(|(position, _)| *position);
            report.count("block_entity.excess_removed", entries.len() - limit);
            entries.truncate(limit);
            region.block_entities.extend(entries);
        }
    }
}

fn process_blocks(
    region: &mut Region,
    region_path: &str,
    policy: &ContentPolicy,
    report: &mut TransformReport,
) {
    let air = BlockState::new("minecraft:air");
    let changed = region.transform_palette_states(|state| {
        let selected = id_action(
            &state.name,
            policy.blocks.allowed_ids.as_ref(),
            &policy.blocks.denied_ids,
            policy.blocks.denied_action,
            policy.allowed_namespaces.as_ref(),
            policy.namespace_action,
        );
        let Some((action, rule)) = selected else {
            return state.clone();
        };
        report.finding(
            "block.policy_match",
            action,
            format!("{region_path}.palette.{}", state.name),
            Some(rule),
        );
        if matches!(action, Action::Remove | Action::Redact) {
            air.clone()
        } else {
            state.clone()
        }
    });
    report.count("block.cells_removed", changed);

    if changed == 0 {
        return;
    }
    let mut entries = region.block_entities.drain();
    entries.sort_by_key(|(position, _)| *position);
    for (position, entity) in entries {
        if region
            .get_block(position.0, position.1, position.2)
            .is_some_and(|state| state.name == "minecraft:air")
        {
            report.finding(
                "block_entity.orphan_removed",
                Action::Remove,
                format!(
                    "{region_path}.block_entities[{},{},{}]",
                    position.0, position.1, position.2
                ),
                Some("block_removed"),
            );
        } else {
            region.block_entities.insert(position, entity);
        }
    }
}

fn prefilter_removed_entities(
    region: &mut Region,
    region_path: &str,
    policy: &ContentPolicy,
    report: &mut TransformReport,
) {
    let mut kept = Vec::with_capacity(region.entities.len());
    for (index, entity) in std::mem::take(&mut region.entities).into_iter().enumerate() {
        let path = format!("{region_path}.entities[{index}]");
        let selected = if entity.id == "minecraft:player" && policy.entities.remove_players {
            Some((Action::Remove, "player_excluded"))
        } else {
            id_action(
                &entity.id,
                policy.entities.allowed_ids.as_ref(),
                &policy.entities.denied_ids,
                policy.entities.denied_action,
                policy.allowed_namespaces.as_ref(),
                policy.namespace_action,
            )
        };
        if let Some((action, rule)) = selected {
            if matches!(action, Action::Remove | Action::Redact) {
                report.finding("entity.policy_match", action, path, Some(rule));
                continue;
            }
        }
        kept.push(entity);
    }
    region.entities = kept;
}

fn prefilter_removed_block_entities(
    region: &mut Region,
    region_path: &str,
    policy: &ContentPolicy,
    report: &mut TransformReport,
) {
    let mut entries = region.block_entities.drain();
    entries.sort_by_key(|(position, _)| *position);
    let mut kept = Vec::with_capacity(entries.len());
    for (position, entity) in entries {
        let path = format!(
            "{region_path}.block_entities[{},{},{}]",
            position.0, position.1, position.2
        );
        let selected = id_action(
            &entity.id,
            policy.block_entities.allowed_ids.as_ref(),
            &policy.block_entities.denied_ids,
            policy.block_entities.denied_action,
            policy.allowed_namespaces.as_ref(),
            policy.namespace_action,
        );
        if let Some((action, rule)) = selected {
            if matches!(action, Action::Remove | Action::Redact) {
                report.finding("block_entity.policy_match", action, path, Some(rule));
                continue;
            }
        }
        kept.push((position, entity));
    }
    region.block_entities.extend(kept);
}

fn total_entities(schematic: &UniversalSchematic) -> usize {
    schematic.default_region.entities.len()
        + schematic
            .other_regions
            .values()
            .map(|region| region.entities.len())
            .sum::<usize>()
}

fn total_block_entities(schematic: &UniversalSchematic) -> usize {
    schematic.default_region.block_entities.len()
        + schematic
            .other_regions
            .values()
            .map(|region| region.block_entities.len())
            .sum::<usize>()
}

fn trim_entities_to(
    schematic: &mut UniversalSchematic,
    maximum: usize,
    report: &mut TransformReport,
) {
    let mut remaining = maximum;
    for_each_region_mut(schematic, |_path, region| {
        let keep = remaining.min(region.entities.len());
        let removed = region.entities.len() - keep;
        region.entities.truncate(keep);
        remaining -= keep;
        report.count("entity.excess_removed", removed);
    });
}

fn trim_block_entities_to(
    schematic: &mut UniversalSchematic,
    maximum: usize,
    report: &mut TransformReport,
) {
    let mut remaining = maximum;
    for_each_region_mut(schematic, |_path, region| {
        let mut entries = region.block_entities.drain();
        entries.sort_by_key(|(position, _)| *position);
        let keep = remaining.min(entries.len());
        let removed = entries.len() - keep;
        entries.truncate(keep);
        remaining -= keep;
        region.block_entities.extend(entries);
        report.count("block_entity.excess_removed", removed);
    });
}

fn assign_missing_uuids(
    schematic: &mut UniversalSchematic,
    policy: &UuidPolicy,
    report: &mut TransformReport,
) {
    if !policy.assign_missing {
        return;
    }
    for_each_region_mut(schematic, |region_path, region| {
        for (index, entity) in region.entities.iter_mut().enumerate() {
            let path = entity_identity_path(
                &region_path,
                index,
                &entity.id,
                entity.position,
                policy.identity_basis,
            );
            assign_entity_map_uuid(&mut entity.nbt, &path, policy, report);
            if let Some(EntityNbt::List(passengers)) = entity.nbt.get_mut("Passengers") {
                assign_passenger_uuids(
                    passengers,
                    &format!("{path}.nbt.Passengers"),
                    policy,
                    report,
                );
            }
        }
    });
}

fn assign_passenger_uuids(
    passengers: &mut [EntityNbt],
    path: &str,
    policy: &UuidPolicy,
    report: &mut TransformReport,
) {
    for (index, passenger) in passengers.iter_mut().enumerate() {
        let EntityNbt::Compound(map) = passenger else {
            continue;
        };
        let entity_path = format!("{path}[{index}]");
        assign_entity_map_uuid(map, &entity_path, policy, report);
        if let Some(EntityNbt::List(nested)) = map.get_mut("Passengers") {
            assign_passenger_uuids(nested, &format!("{entity_path}.Passengers"), policy, report);
        }
    }
}

fn assign_entity_map_uuid(
    map: &mut HashMap<String, EntityNbt>,
    path: &str,
    policy: &UuidPolicy,
    report: &mut TransformReport,
) {
    let has_definition = map.keys().any(|key| {
        uuid_role(key, policy) == Some(UuidRole::Definition)
            || key
                .strip_suffix("Most")
                .or_else(|| key.strip_suffix("MSB"))
                .is_some_and(|base| uuid_role(base, policy) == Some(UuidRole::Definition))
    });
    if has_definition {
        return;
    }
    let uuid = match policy.mode {
        UuidMode::RegenerateDeterministic => Uuid128::deterministic_identity(path, &policy.salt),
        UuidMode::RegenerateRandom => Uuid128::random(),
        _ => return,
    };
    let key = policy
        .definition_keys
        .iter()
        .find(|key| key.as_str() == "UUID")
        .cloned()
        .or_else(|| policy.definition_keys.iter().next().cloned())
        .unwrap_or_else(|| "UUID".to_string());
    match policy.representation {
        UuidRepresentation::String => {
            map.insert(key, EntityNbt::String(uuid.canonical()));
        }
        UuidRepresentation::LongPair => {
            let (most, least) = uuid.longs();
            map.insert(format!("{key}Most"), EntityNbt::Long(most));
            map.insert(format!("{key}Least"), EntityNbt::Long(least));
        }
        UuidRepresentation::IntArray | UuidRepresentation::Preserve => {
            map.insert(key, EntityNbt::IntArray(uuid.ints()));
        }
    }
    report.finding(
        "uuid.assigned",
        Action::Redact,
        path,
        Some("assign_missing"),
    );
}

fn id_action(
    id: &str,
    allowed: Option<&BTreeSet<String>>,
    denied: &BTreeSet<String>,
    denied_action: Action,
    allowed_namespaces: Option<&BTreeSet<String>>,
    namespace_action: Action,
) -> Option<(Action, &'static str)> {
    if denied.contains(id) || allowed.is_some_and(|set| !set.contains(id)) {
        return Some((denied_action, "id_not_allowed"));
    }
    if allowed_namespaces.is_some_and(|set| !set.contains(namespace(id))) {
        return Some((namespace_action, "namespace_not_allowed"));
    }
    None
}

fn process_entities(
    region: &mut Region,
    region_path: &str,
    policy: &ContentPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let mut kept = Vec::with_capacity(region.entities.len());
    for (index, mut entity) in std::mem::take(&mut region.entities).into_iter().enumerate() {
        let path = format!("{region_path}.entities[{index}]");
        let player = entity.id == "minecraft:player";
        let selected = if player && policy.entities.remove_players {
            Some((Action::Remove, "player_excluded"))
        } else {
            id_action(
                &entity.id,
                policy.entities.allowed_ids.as_ref(),
                &policy.entities.denied_ids,
                policy.entities.denied_action,
                policy.allowed_namespaces.as_ref(),
                policy.namespace_action,
            )
        };
        if let Some((action, rule)) = selected {
            report.finding("entity.policy_match", action, &path, Some(rule));
            if action == Action::Remove || action == Action::Redact {
                continue;
            }
        }
        let mut budget = VisitBudget::default();
        process_entity_map(
            &mut entity.nbt,
            &format!("{path}.nbt"),
            policy,
            uuid_map,
            &mut budget,
            report,
        );
        kept.push(entity);
    }
    region.entities = kept;
}

fn process_block_entities(
    region: &mut Region,
    region_path: &str,
    policy: &ContentPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let mut entries = region.block_entities.drain();
    entries.sort_by_key(|(position, _)| *position);
    let mut kept = Vec::with_capacity(entries.len());
    for (position, mut entity) in entries {
        let path = format!(
            "{region_path}.block_entities[{},{},{}]",
            position.0, position.1, position.2
        );
        if let Some((action, rule)) = id_action(
            &entity.id,
            policy.block_entities.allowed_ids.as_ref(),
            &policy.block_entities.denied_ids,
            policy.block_entities.denied_action,
            policy.allowed_namespaces.as_ref(),
            policy.namespace_action,
        ) {
            report.finding("block_entity.policy_match", action, &path, Some(rule));
            if action == Action::Remove || action == Action::Redact {
                continue;
            }
        }
        let mut map = (*entity.nbt).clone();
        let mut budget = VisitBudget::default();
        process_block_map(
            &mut map,
            &format!("{path}.nbt"),
            policy,
            uuid_map,
            &mut budget,
            report,
        );
        entity.set_nbt(map);
        kept.push((position, entity));
    }
    region.block_entities.extend(kept);
}

#[derive(Default)]
struct VisitBudget {
    nodes: usize,
}

fn nbt_depth(path: &str) -> usize {
    let suffix = path
        .split_once(".nbt")
        .map(|(_, suffix)| suffix)
        .unwrap_or(path);
    suffix.matches('.').count() + suffix.matches('[').count()
}

fn check_budget(
    path: &str,
    depth: usize,
    collection_len: usize,
    policy: &ContentPolicy,
    budget: &mut VisitBudget,
    report: &mut TransformReport,
) {
    budget.nodes += 1;
    if depth > policy.nbt.max_depth {
        report.finding(
            "nbt.depth_limit_exceeded",
            policy.nbt.limit_action,
            path,
            Some("max_depth"),
        );
    }
    if let Some(maximum) = policy.nbt.max_nodes {
        if budget.nodes == maximum.saturating_add(1) {
            report.finding(
                "nbt.node_limit_exceeded",
                policy.nbt.limit_action,
                path,
                Some("max_nodes"),
            );
        }
    }
    if policy
        .nbt
        .max_collection_items
        .is_some_and(|maximum| collection_len > maximum)
    {
        report.finding(
            "nbt.collection_limit_exceeded",
            policy.nbt.limit_action,
            path,
            Some("max_collection_items"),
        );
    }
}

fn process_text(value: &mut String, path: &str, policy: &TextPolicy, report: &mut TransformReport) {
    let lower = value.to_lowercase();
    let mut suspicious = false;
    for pattern in &policy.suspicious_patterns {
        if lower.contains(&pattern.to_lowercase()) {
            suspicious = true;
            report.finding(
                "text.suspicious_pattern",
                policy.suspicious_action,
                path,
                Some("suspicious_patterns"),
            );
        }
    }
    if suspicious && matches!(policy.suspicious_action, Action::Redact | Action::Remove) {
        value.clear();
        value.push_str(&policy.redaction);
    }
    for word in &policy.redact_words {
        if word.is_empty() {
            continue;
        }
        let needle = word.to_lowercase();
        while let Some(start) = value.to_lowercase().find(&needle) {
            let end = start + word.len();
            if value.is_char_boundary(start) && value.is_char_boundary(end) {
                value.replace_range(start..end, &policy.redaction);
                report.finding("text.redacted", Action::Redact, path, Some("redact_words"));
            } else {
                break;
            }
        }
    }
    if let Some(maximum) = policy.max_string_bytes {
        if value.len() > maximum {
            report.finding(
                "text.size_limit_exceeded",
                policy.oversize_action,
                path,
                Some("max_string_bytes"),
            );
            if matches!(policy.oversize_action, Action::Redact | Action::Remove) {
                value.clear();
                value.push_str(&policy.redaction);
            }
        }
    }
}

fn process_block_map(
    map: &mut NbtMap,
    path: &str,
    policy: &ContentPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    budget: &mut VisitBudget,
    report: &mut TransformReport,
) {
    check_budget(
        path,
        nbt_depth(path),
        map.inner().len(),
        policy,
        budget,
        report,
    );
    rewrite_block_uuid_fields(map, path, &policy.uuids, uuid_map, report);
    let mut keys: Vec<String> = map.iter().map(|(key, _)| key.clone()).collect();
    keys.sort();
    for key in keys {
        let child = format!("{path}.{key}");
        if let Some((code, action, rule)) = nbt_key_action(&key, policy) {
            report.finding(code, action, &child, Some(rule));
            if matches!(action, Action::Remove | Action::Redact) {
                map.remove(&key);
                continue;
            }
        }
        if let Some(value) = map.get_mut(&key) {
            process_block_value(value, &child, policy, uuid_map, budget, report);
        }
    }
}

fn process_block_value(
    value: &mut NbtValue,
    path: &str,
    policy: &ContentPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    budget: &mut VisitBudget,
    report: &mut TransformReport,
) {
    match value {
        NbtValue::String(text) => process_text(text, path, &policy.text, report),
        NbtValue::Compound(map) => {
            if item_id_block(map).is_some_and(|id| item_id_disallowed(id, &policy.items)) {
                report.finding(
                    "item.denied",
                    policy.items.denied_action,
                    path,
                    Some("denied_ids"),
                );
            }
            process_block_map(map, path, policy, uuid_map, budget, report)
        }
        NbtValue::List(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report);
            if policy.items.clear_inventories && path.ends_with(".Items") {
                let removed = values.len();
                values.clear();
                report.count("inventory.items_removed", removed);
                return;
            }
            enforce_inventory_budget(values, path, &policy.items, report);
            let mut next = Vec::with_capacity(values.len());
            for (index, mut child) in std::mem::take(values).into_iter().enumerate() {
                let child_path = format!("{path}[{index}]");
                let denied_item = match &child {
                    NbtValue::Compound(map) => {
                        item_id_block(map).is_some_and(|id| item_id_disallowed(id, &policy.items))
                    }
                    _ => false,
                };
                if denied_item {
                    report.finding(
                        "item.denied",
                        policy.items.denied_action,
                        &child_path,
                        Some("denied_ids"),
                    );
                    if matches!(policy.items.denied_action, Action::Remove | Action::Redact) {
                        continue;
                    }
                }
                process_block_value(&mut child, &child_path, policy, uuid_map, budget, report);
                next.push(child);
            }
            *values = next;
        }
        NbtValue::ByteArray(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report)
        }
        NbtValue::IntArray(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report)
        }
        NbtValue::LongArray(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report)
        }
        _ => {}
    }
}

fn process_entity_map(
    map: &mut HashMap<String, EntityNbt>,
    path: &str,
    policy: &ContentPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    budget: &mut VisitBudget,
    report: &mut TransformReport,
) {
    check_budget(path, nbt_depth(path), map.len(), policy, budget, report);
    rewrite_entity_uuid_fields(map, path, &policy.uuids, uuid_map, report);
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let child = format!("{path}.{key}");
        if let Some((code, action, rule)) = nbt_key_action(&key, policy) {
            report.finding(code, action, &child, Some(rule));
            if matches!(action, Action::Remove | Action::Redact) {
                map.remove(&key);
                continue;
            }
        }
        if let Some(value) = map.get_mut(&key) {
            process_entity_value(value, &child, policy, uuid_map, budget, report);
        }
    }
}

fn nbt_key_action<'a>(
    key: &'a str,
    policy: &ContentPolicy,
) -> Option<(&'static str, Action, &'a str)> {
    if policy.text.strip_keys.contains(key) {
        Some(("text.field_removed", Action::Remove, key))
    } else if policy.nbt.remove_keys.contains(key) {
        Some(("nbt.field_removed", Action::Remove, key))
    } else if policy.nbt.executable_keys.contains(key) {
        Some(("nbt.executable_field", policy.nbt.executable_action, key))
    } else if policy.nbt.profile_keys.contains(key) {
        Some(("nbt.profile_field", policy.nbt.profile_action, key))
    } else if policy.nbt.volatile_keys.contains(key) {
        Some(("nbt.volatile_field", policy.nbt.volatile_action, key))
    } else {
        None
    }
}

fn process_entity_value(
    value: &mut EntityNbt,
    path: &str,
    policy: &ContentPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    budget: &mut VisitBudget,
    report: &mut TransformReport,
) {
    match value {
        EntityNbt::String(text) => process_text(text, path, &policy.text, report),
        EntityNbt::Compound(map) => {
            if item_id_entity(map).is_some_and(|id| item_id_disallowed(id, &policy.items)) {
                report.finding(
                    "item.denied",
                    policy.items.denied_action,
                    path,
                    Some("denied_ids"),
                );
            }
            process_entity_map(map, path, policy, uuid_map, budget, report)
        }
        EntityNbt::List(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report);
            if policy.items.clear_inventories
                && (path.ends_with(".Items") || path.ends_with(".Inventory"))
            {
                let removed = values.len();
                values.clear();
                report.count("inventory.items_removed", removed);
                return;
            }
            enforce_entity_inventory_budget(values, path, &policy.items, report);
            let mut next = Vec::with_capacity(values.len());
            for (index, mut child) in std::mem::take(values).into_iter().enumerate() {
                let child_path = format!("{path}[{index}]");
                let denied_item = match &child {
                    EntityNbt::Compound(map) => {
                        item_id_entity(map).is_some_and(|id| item_id_disallowed(id, &policy.items))
                    }
                    _ => false,
                };
                if denied_item {
                    report.finding(
                        "item.denied",
                        policy.items.denied_action,
                        &child_path,
                        Some("denied_ids"),
                    );
                    if matches!(policy.items.denied_action, Action::Remove | Action::Redact) {
                        continue;
                    }
                }
                process_entity_value(&mut child, &child_path, policy, uuid_map, budget, report);
                next.push(child);
            }
            *values = next;
        }
        EntityNbt::ByteArray(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report)
        }
        EntityNbt::IntArray(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report)
        }
        EntityNbt::LongArray(values) => {
            check_budget(path, nbt_depth(path), values.len(), policy, budget, report)
        }
        _ => {}
    }
}

fn item_id_block(map: &NbtMap) -> Option<&str> {
    map.get("id")
        .or_else(|| map.get("Id"))
        .and_then(NbtValue::as_string)
        .map(String::as_str)
}

fn item_id_entity(map: &HashMap<String, EntityNbt>) -> Option<&str> {
    match map.get("id").or_else(|| map.get("Id")) {
        Some(EntityNbt::String(id)) => Some(id),
        _ => None,
    }
}

fn item_id_disallowed(id: &str, policy: &ItemPolicy) -> bool {
    policy.denied_ids.contains(id)
        || policy
            .allowed_ids
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(id))
}

fn enforce_inventory_budget(
    values: &mut Vec<NbtValue>,
    path: &str,
    policy: &ItemPolicy,
    report: &mut TransformReport,
) {
    if !path.ends_with(".Items") && !path.ends_with(".Inventory") {
        return;
    }
    let Some(limit) = policy.max_inventory_items else {
        return;
    };
    if values.len() <= limit {
        return;
    }
    report.finding(
        "inventory.item_limit_exceeded",
        policy.excess_action,
        path,
        Some("max_inventory_items"),
    );
    if matches!(policy.excess_action, Action::Remove | Action::Redact) {
        report.count("inventory.items_removed", values.len() - limit);
        values.truncate(limit);
    }
}

fn enforce_entity_inventory_budget(
    values: &mut Vec<EntityNbt>,
    path: &str,
    policy: &ItemPolicy,
    report: &mut TransformReport,
) {
    if !path.ends_with(".Items") && !path.ends_with(".Inventory") {
        return;
    }
    let Some(limit) = policy.max_inventory_items else {
        return;
    };
    if values.len() <= limit {
        return;
    }
    report.finding(
        "inventory.item_limit_exceeded",
        policy.excess_action,
        path,
        Some("max_inventory_items"),
    );
    if matches!(policy.excess_action, Action::Remove | Action::Redact) {
        report.count("inventory.items_removed", values.len() - limit);
        values.truncate(limit);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Uuid128([u8; 16]);

impl Uuid128 {
    fn from_ints(values: &[i32]) -> Option<Self> {
        if values.len() != 4 {
            return None;
        }
        let mut bytes = [0; 16];
        for (index, value) in values.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
        }
        Some(Self(bytes))
    }

    fn from_longs(most: i64, least: i64) -> Self {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&most.to_be_bytes());
        bytes[8..].copy_from_slice(&least.to_be_bytes());
        Self(bytes)
    }

    fn parse(text: &str) -> Option<Self> {
        let hex: String = text.chars().filter(|character| *character != '-').collect();
        if hex.len() != 32 {
            return None;
        }
        let mut bytes = [0; 16];
        for index in 0..16 {
            bytes[index] = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }

    fn ints(self) -> Vec<i32> {
        self.0
            .chunks_exact(4)
            .map(|part| i32::from_be_bytes(part.try_into().unwrap()))
            .collect()
    }

    fn longs(self) -> (i64, i64) {
        (
            i64::from_be_bytes(self.0[..8].try_into().unwrap()),
            i64::from_be_bytes(self.0[8..].try_into().unwrap()),
        )
    }

    fn canonical(self) -> String {
        let hex = self
            .0
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }

    fn deterministic_identity(identity: &str, salt: &str) -> Self {
        let mut input = Vec::with_capacity(salt.len() + identity.len() + 1);
        input.extend_from_slice(salt.as_bytes());
        input.push(0);
        input.extend_from_slice(identity.as_bytes());
        let digest = blake3::hash(&input);
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x50;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self(bytes)
    }

    fn random() -> Self {
        let mut bytes = [0; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UuidRole {
    Definition,
    Reference,
}

fn uuid_role(key: &str, policy: &UuidPolicy) -> Option<UuidRole> {
    let base = key
        .strip_suffix("Most")
        .or_else(|| key.strip_suffix("Least"))
        .or_else(|| key.strip_suffix("MSB"))
        .or_else(|| key.strip_suffix("LSB"))
        .unwrap_or(key);
    if policy.definition_keys.contains(base) {
        Some(UuidRole::Definition)
    } else if policy.reference_keys.contains(base) {
        Some(UuidRole::Reference)
    } else if policy.scope == UuidScope::AllRecognized
        && (base.to_ascii_lowercase().contains("uuid") || base.ends_with("Owner"))
    {
        Some(UuidRole::Reference)
    } else {
        None
    }
}

fn uuid_role_at(key: &str, path: &str, policy: &UuidPolicy) -> Option<UuidRole> {
    let role = uuid_role(key, policy)?;
    if role == UuidRole::Definition
        && [".Owner", ".owner", ".Leash", ".leash"]
            .iter()
            .any(|segment| path.ends_with(segment))
    {
        Some(UuidRole::Reference)
    } else {
        Some(role)
    }
}

fn build_uuid_map(
    schematic: &UniversalSchematic,
    policy: &UuidPolicy,
    report: &mut TransformReport,
) -> BTreeMap<Uuid128, Uuid128> {
    let mut definitions = Vec::new();
    collect_schematic_uuids(schematic, policy, &mut definitions);
    let mut counts: BTreeMap<Uuid128, usize> = BTreeMap::new();
    for (uuid, role, path) in &definitions {
        if *role == UuidRole::Definition {
            let count = counts.entry(*uuid).or_default();
            *count += 1;
            if *count > 1 {
                let action = match policy.collision {
                    CollisionPolicy::Reject => Action::Reject,
                    CollisionPolicy::Warn => Action::Warn,
                    CollisionPolicy::Keep => Action::Allow,
                };
                report.finding("uuid.definition_collision", action, path, Some("collision"));
            }
        }
    }
    let mut mapping = BTreeMap::new();
    let mut targets = BTreeMap::<Uuid128, Uuid128>::new();
    for (uuid, role, path) in &definitions {
        // References follow the mapping created for their definition. Unknown
        // references remain dangling and are handled by `target_uuid`; giving
        // them an independent replacement would silently break graph identity.
        if *role != UuidRole::Definition || mapping.contains_key(uuid) {
            continue;
        }
        let target = match policy.mode {
            // Path-based identity makes this pass idempotent: after the first
            // rewrite, the same definition at the same canonical path maps to
            // the same value rather than hashing its replacement again.
            UuidMode::RegenerateDeterministic => {
                Uuid128::deterministic_identity(path, &policy.salt)
            }
            UuidMode::RegenerateRandom => Uuid128::random(),
            _ => *uuid,
        };
        if let Some(previous) = targets.insert(target, *uuid) {
            if previous != *uuid {
                let action = match policy.collision {
                    CollisionPolicy::Reject => Action::Reject,
                    CollisionPolicy::Warn => Action::Warn,
                    CollisionPolicy::Keep => Action::Allow,
                };
                report.finding(
                    "uuid.generated_collision",
                    action,
                    path,
                    Some("identity_basis"),
                );
            }
        }
        mapping.insert(*uuid, target);
    }
    mapping
}

fn collect_schematic_uuids(
    schematic: &UniversalSchematic,
    policy: &UuidPolicy,
    out: &mut Vec<(Uuid128, UuidRole, String)>,
) {
    let collect_region =
        |name: &str, region: &Region, out: &mut Vec<(Uuid128, UuidRole, String)>| {
            for (index, entity) in region.entities.iter().enumerate() {
                let identity = entity_identity_path(
                    &format!("regions.{name}"),
                    index,
                    &entity.id,
                    entity.position,
                    policy.identity_basis,
                );
                collect_entity_map_uuids(&entity.nbt, &format!("{identity}.nbt"), policy, out);
            }
            let mut block_entities: Vec<_> = region.block_entities.iter().collect();
            block_entities.sort_by_key(|(position, _)| *position);
            for (position, entity) in block_entities {
                collect_block_map_uuids(
                    &entity.nbt,
                    &format!(
                        "regions.{name}.block_entities[{},{},{}].nbt",
                        position.0, position.1, position.2
                    ),
                    policy,
                    out,
                );
            }
        };
    collect_region(
        &schematic.default_region_name,
        &schematic.default_region,
        out,
    );
    let mut names: Vec<_> = schematic.other_regions.keys().collect();
    names.sort();
    for name in names {
        collect_region(name, &schematic.other_regions[name], out);
    }
}

fn entity_identity_path(
    region_path: &str,
    index: usize,
    entity_id: &str,
    position: (f64, f64, f64),
    basis: UuidIdentityBasis,
) -> String {
    match basis {
        UuidIdentityBasis::StablePath => format!("{region_path}.entities[{index}]"),
        UuidIdentityBasis::EntityLocation => format!(
            "{region_path}.entity[id={},pos={:016x},{:016x},{:016x}]",
            entity_id,
            position.0.to_bits(),
            position.1.to_bits(),
            position.2.to_bits(),
        ),
    }
}

fn collect_block_map_uuids(
    map: &NbtMap,
    path: &str,
    policy: &UuidPolicy,
    out: &mut Vec<(Uuid128, UuidRole, String)>,
) {
    collect_block_local_uuids(map, path, policy, out);
    let mut keys: Vec<_> = map.iter().collect();
    keys.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in keys {
        collect_block_value_uuids(value, &format!("{path}.{key}"), policy, out);
    }
}

fn collect_block_value_uuids(
    value: &NbtValue,
    path: &str,
    policy: &UuidPolicy,
    out: &mut Vec<(Uuid128, UuidRole, String)>,
) {
    match value {
        NbtValue::Compound(map) => collect_block_map_uuids(map, path, policy, out),
        NbtValue::List(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_block_value_uuids(value, &format!("{path}[{index}]"), policy, out);
            }
        }
        _ => {}
    }
}

fn collect_block_local_uuids(
    map: &NbtMap,
    path: &str,
    policy: &UuidPolicy,
    out: &mut Vec<(Uuid128, UuidRole, String)>,
) {
    for (key, value) in map.iter() {
        if let Some(role) = uuid_role_at(key, path, policy) {
            if let Some(uuid) = parse_block_uuid(value) {
                out.push((uuid, role, format!("{path}.{key}")));
            } else if let NbtValue::List(values) = value {
                for (index, value) in values.iter().enumerate() {
                    if let Some(uuid) = parse_block_uuid(value) {
                        out.push((uuid, role, format!("{path}.{key}[{index}]")));
                    }
                }
            }
        }
        for suffixes in [("Most", "Least"), ("MSB", "LSB")] {
            if let Some(base) = key.strip_suffix(suffixes.0) {
                let Some(role) = uuid_role_at(base, path, policy) else {
                    continue;
                };
                let Some(NbtValue::Long(most)) = map.get(key) else {
                    continue;
                };
                let Some(NbtValue::Long(least)) = map.get(&format!("{base}{}", suffixes.1)) else {
                    continue;
                };
                out.push((
                    Uuid128::from_longs(*most, *least),
                    role,
                    format!("{path}.{base}"),
                ));
            }
        }
    }
}

fn collect_entity_map_uuids(
    map: &HashMap<String, EntityNbt>,
    path: &str,
    policy: &UuidPolicy,
    out: &mut Vec<(Uuid128, UuidRole, String)>,
) {
    collect_entity_local_uuids(map, path, policy, out);
    let mut keys: Vec<_> = map.iter().collect();
    keys.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in keys {
        collect_entity_value_uuids(value, &format!("{path}.{key}"), policy, out);
    }
}

fn collect_entity_value_uuids(
    value: &EntityNbt,
    path: &str,
    policy: &UuidPolicy,
    out: &mut Vec<(Uuid128, UuidRole, String)>,
) {
    match value {
        EntityNbt::Compound(map) => collect_entity_map_uuids(map, path, policy, out),
        EntityNbt::List(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_entity_value_uuids(value, &format!("{path}[{index}]"), policy, out);
            }
        }
        _ => {}
    }
}

fn collect_entity_local_uuids(
    map: &HashMap<String, EntityNbt>,
    path: &str,
    policy: &UuidPolicy,
    out: &mut Vec<(Uuid128, UuidRole, String)>,
) {
    for (key, value) in map {
        if let Some(role) = uuid_role_at(key, path, policy) {
            if let Some(uuid) = parse_entity_uuid(value) {
                out.push((uuid, role, format!("{path}.{key}")));
            } else if let EntityNbt::List(values) = value {
                for (index, value) in values.iter().enumerate() {
                    if let Some(uuid) = parse_entity_uuid(value) {
                        out.push((uuid, role, format!("{path}.{key}[{index}]")));
                    }
                }
            }
        }
        for suffixes in [("Most", "Least"), ("MSB", "LSB")] {
            if let Some(base) = key.strip_suffix(suffixes.0) {
                let Some(role) = uuid_role_at(base, path, policy) else {
                    continue;
                };
                let Some(EntityNbt::Long(most)) = map.get(key) else {
                    continue;
                };
                let Some(EntityNbt::Long(least)) = map.get(&format!("{base}{}", suffixes.1)) else {
                    continue;
                };
                out.push((
                    Uuid128::from_longs(*most, *least),
                    role,
                    format!("{path}.{base}"),
                ));
            }
        }
    }
}

fn parse_block_uuid(value: &NbtValue) -> Option<Uuid128> {
    match value {
        NbtValue::IntArray(values) => Uuid128::from_ints(values),
        NbtValue::String(value) => Uuid128::parse(value),
        _ => None,
    }
}

fn parse_entity_uuid(value: &EntityNbt) -> Option<Uuid128> {
    match value {
        EntityNbt::IntArray(values) => Uuid128::from_ints(values),
        EntityNbt::String(value) => Uuid128::parse(value),
        _ => None,
    }
}

fn should_rewrite(role: UuidRole, policy: &UuidPolicy) -> bool {
    role == UuidRole::Definition || policy.scope != UuidScope::DefinitionsOnly
}

fn target_uuid(
    source: Uuid128,
    role: UuidRole,
    path: &str,
    policy: &UuidPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) -> Option<Uuid128> {
    if role == UuidRole::Reference && !uuid_map.contains_key(&source) {
        let action = match policy.dangling {
            DanglingReferencePolicy::Warn => Action::Warn,
            DanglingReferencePolicy::Remove => Action::Remove,
            DanglingReferencePolicy::Reject => Action::Reject,
            DanglingReferencePolicy::Preserve => Action::Allow,
        };
        report.finding("uuid.dangling_reference", action, path, Some("dangling"));
        if policy.dangling == DanglingReferencePolicy::Remove {
            return None;
        }
    }
    uuid_map.get(&source).copied().or(Some(source))
}

fn rewrite_block_uuid_fields(
    map: &mut NbtMap,
    path: &str,
    policy: &UuidPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let keys: Vec<String> = map.iter().map(|(key, _)| key.clone()).collect();
    for key in keys {
        let Some(role) = uuid_role_at(&key, path, policy) else {
            continue;
        };
        if matches!(map.get(&key), Some(NbtValue::List(_))) {
            if !should_rewrite(role, policy) {
                continue;
            }
            let field_path = format!("{path}.{key}");
            if policy.mode == UuidMode::Remove {
                map.remove(&key);
                report.finding("uuid.removed", Action::Remove, field_path, Some("mode"));
            } else if let Some(NbtValue::List(values)) = map.get_mut(&key) {
                rewrite_block_uuid_list(values, &field_path, role, policy, uuid_map, report);
            }
            continue;
        }
        let Some(source) = map.get(&key).and_then(parse_block_uuid) else {
            continue;
        };
        if !should_rewrite(role, policy) {
            continue;
        }
        let field_path = format!("{path}.{key}");
        if policy.mode == UuidMode::Remove {
            map.remove(&key);
            report.finding("uuid.removed", Action::Remove, field_path, Some("mode"));
            continue;
        }
        let Some(target) = target_uuid(source, role, &field_path, policy, uuid_map, report) else {
            map.remove(&key);
            continue;
        };
        let representation = if policy.representation == UuidRepresentation::Preserve {
            match map.get(&key) {
                Some(NbtValue::String(_)) => UuidRepresentation::String,
                _ => UuidRepresentation::IntArray,
            }
        } else {
            policy.representation
        };
        match representation {
            UuidRepresentation::String => {
                map.insert(key.clone(), NbtValue::String(target.canonical()));
            }
            UuidRepresentation::IntArray => {
                map.insert(key.clone(), NbtValue::IntArray(target.ints()));
            }
            UuidRepresentation::LongPair => {
                map.remove(&key);
                let (most, least) = target.longs();
                map.insert(format!("{key}Most"), NbtValue::Long(most));
                map.insert(format!("{key}Least"), NbtValue::Long(least));
            }
            UuidRepresentation::Preserve => unreachable!(),
        }
        if source != target || policy.representation != UuidRepresentation::Preserve {
            report.finding(
                "uuid.rewritten",
                Action::Redact,
                field_path,
                Some("uuid_policy"),
            );
        }
    }
    rewrite_block_long_pairs(map, path, policy, uuid_map, report);
}

fn rewrite_block_long_pairs(
    map: &mut NbtMap,
    path: &str,
    policy: &UuidPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let keys: Vec<String> = map.iter().map(|(key, _)| key.clone()).collect();
    for most_key in keys {
        let (base, least_suffix) = if let Some(base) = most_key.strip_suffix("Most") {
            (base.to_string(), "Least")
        } else if let Some(base) = most_key.strip_suffix("MSB") {
            (base.to_string(), "LSB")
        } else {
            continue;
        };
        let Some(role) = uuid_role_at(&base, path, policy) else {
            continue;
        };
        if !should_rewrite(role, policy) {
            continue;
        }
        let least_key = format!("{base}{least_suffix}");
        let (Some(NbtValue::Long(most)), Some(NbtValue::Long(least))) =
            (map.get(&most_key), map.get(&least_key))
        else {
            continue;
        };
        let source = Uuid128::from_longs(*most, *least);
        let field_path = format!("{path}.{base}");
        map.remove(&most_key);
        map.remove(&least_key);
        if policy.mode == UuidMode::Remove {
            report.finding("uuid.removed", Action::Remove, field_path, Some("mode"));
            continue;
        }
        let Some(target) = target_uuid(source, role, &field_path, policy, uuid_map, report) else {
            continue;
        };
        match policy.representation {
            UuidRepresentation::String => {
                map.insert(base, NbtValue::String(target.canonical()));
            }
            UuidRepresentation::IntArray => {
                map.insert(base, NbtValue::IntArray(target.ints()));
            }
            UuidRepresentation::LongPair | UuidRepresentation::Preserve => {
                let (most, least) = target.longs();
                map.insert(most_key, NbtValue::Long(most));
                map.insert(least_key, NbtValue::Long(least));
            }
        }
        if source != target || policy.representation != UuidRepresentation::Preserve {
            report.finding(
                "uuid.rewritten",
                Action::Redact,
                field_path,
                Some("uuid_policy"),
            );
        }
    }
}

fn rewrite_block_uuid_list(
    values: &mut Vec<NbtValue>,
    path: &str,
    role: UuidRole,
    policy: &UuidPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let mut next = Vec::with_capacity(values.len());
    for (index, value) in std::mem::take(values).into_iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let Some(source) = parse_block_uuid(&value) else {
            next.push(value);
            continue;
        };
        let Some(target) = target_uuid(source, role, &item_path, policy, uuid_map, report) else {
            continue;
        };
        let rewritten = match policy.representation {
            UuidRepresentation::String => NbtValue::String(target.canonical()),
            UuidRepresentation::IntArray | UuidRepresentation::LongPair => {
                if policy.representation == UuidRepresentation::LongPair {
                    report.finding(
                        "uuid.list_long_pair_fallback",
                        Action::Warn,
                        &item_path,
                        Some("representation"),
                    );
                }
                NbtValue::IntArray(target.ints())
            }
            UuidRepresentation::Preserve => match value {
                NbtValue::String(_) => NbtValue::String(target.canonical()),
                _ => NbtValue::IntArray(target.ints()),
            },
        };
        if source != target || policy.representation != UuidRepresentation::Preserve {
            report.finding(
                "uuid.rewritten",
                Action::Redact,
                item_path,
                Some("uuid_policy"),
            );
        }
        next.push(rewritten);
    }
    *values = next;
}

fn rewrite_entity_uuid_fields(
    map: &mut HashMap<String, EntityNbt>,
    path: &str,
    policy: &UuidPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        let Some(role) = uuid_role_at(&key, path, policy) else {
            continue;
        };
        if matches!(map.get(&key), Some(EntityNbt::List(_))) {
            if !should_rewrite(role, policy) {
                continue;
            }
            let field_path = format!("{path}.{key}");
            if policy.mode == UuidMode::Remove {
                map.remove(&key);
                report.finding("uuid.removed", Action::Remove, field_path, Some("mode"));
            } else if let Some(EntityNbt::List(values)) = map.get_mut(&key) {
                rewrite_entity_uuid_list(values, &field_path, role, policy, uuid_map, report);
            }
            continue;
        }
        let Some(source) = map.get(&key).and_then(parse_entity_uuid) else {
            continue;
        };
        if !should_rewrite(role, policy) {
            continue;
        }
        let field_path = format!("{path}.{key}");
        if policy.mode == UuidMode::Remove {
            map.remove(&key);
            report.finding("uuid.removed", Action::Remove, field_path, Some("mode"));
            continue;
        }
        let Some(target) = target_uuid(source, role, &field_path, policy, uuid_map, report) else {
            map.remove(&key);
            continue;
        };
        let representation = if policy.representation == UuidRepresentation::Preserve {
            match map.get(&key) {
                Some(EntityNbt::String(_)) => UuidRepresentation::String,
                _ => UuidRepresentation::IntArray,
            }
        } else {
            policy.representation
        };
        match representation {
            UuidRepresentation::String => {
                map.insert(key.clone(), EntityNbt::String(target.canonical()));
            }
            UuidRepresentation::IntArray => {
                map.insert(key.clone(), EntityNbt::IntArray(target.ints()));
            }
            UuidRepresentation::LongPair => {
                map.remove(&key);
                let (most, least) = target.longs();
                map.insert(format!("{key}Most"), EntityNbt::Long(most));
                map.insert(format!("{key}Least"), EntityNbt::Long(least));
            }
            UuidRepresentation::Preserve => unreachable!(),
        }
        if source != target || policy.representation != UuidRepresentation::Preserve {
            report.finding(
                "uuid.rewritten",
                Action::Redact,
                field_path,
                Some("uuid_policy"),
            );
        }
    }
    rewrite_entity_long_pairs(map, path, policy, uuid_map, report);
}

fn rewrite_entity_long_pairs(
    map: &mut HashMap<String, EntityNbt>,
    path: &str,
    policy: &UuidPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let keys: Vec<String> = map.keys().cloned().collect();
    for most_key in keys {
        let (base, least_suffix) = if let Some(base) = most_key.strip_suffix("Most") {
            (base.to_string(), "Least")
        } else if let Some(base) = most_key.strip_suffix("MSB") {
            (base.to_string(), "LSB")
        } else {
            continue;
        };
        let Some(role) = uuid_role_at(&base, path, policy) else {
            continue;
        };
        if !should_rewrite(role, policy) {
            continue;
        }
        let least_key = format!("{base}{least_suffix}");
        let (Some(EntityNbt::Long(most)), Some(EntityNbt::Long(least))) =
            (map.get(&most_key), map.get(&least_key))
        else {
            continue;
        };
        let source = Uuid128::from_longs(*most, *least);
        let field_path = format!("{path}.{base}");
        map.remove(&most_key);
        map.remove(&least_key);
        if policy.mode == UuidMode::Remove {
            report.finding("uuid.removed", Action::Remove, field_path, Some("mode"));
            continue;
        }
        let Some(target) = target_uuid(source, role, &field_path, policy, uuid_map, report) else {
            continue;
        };
        match policy.representation {
            UuidRepresentation::String => {
                map.insert(base, EntityNbt::String(target.canonical()));
            }
            UuidRepresentation::IntArray => {
                map.insert(base, EntityNbt::IntArray(target.ints()));
            }
            UuidRepresentation::LongPair | UuidRepresentation::Preserve => {
                let (most, least) = target.longs();
                map.insert(most_key, EntityNbt::Long(most));
                map.insert(least_key, EntityNbt::Long(least));
            }
        }
        if source != target || policy.representation != UuidRepresentation::Preserve {
            report.finding(
                "uuid.rewritten",
                Action::Redact,
                field_path,
                Some("uuid_policy"),
            );
        }
    }
}

fn rewrite_entity_uuid_list(
    values: &mut Vec<EntityNbt>,
    path: &str,
    role: UuidRole,
    policy: &UuidPolicy,
    uuid_map: &BTreeMap<Uuid128, Uuid128>,
    report: &mut TransformReport,
) {
    let mut next = Vec::with_capacity(values.len());
    for (index, value) in std::mem::take(values).into_iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let Some(source) = parse_entity_uuid(&value) else {
            next.push(value);
            continue;
        };
        let Some(target) = target_uuid(source, role, &item_path, policy, uuid_map, report) else {
            continue;
        };
        let rewritten = match policy.representation {
            UuidRepresentation::String => EntityNbt::String(target.canonical()),
            UuidRepresentation::IntArray | UuidRepresentation::LongPair => {
                if policy.representation == UuidRepresentation::LongPair {
                    report.finding(
                        "uuid.list_long_pair_fallback",
                        Action::Warn,
                        &item_path,
                        Some("representation"),
                    );
                }
                EntityNbt::IntArray(target.ints())
            }
            UuidRepresentation::Preserve => match value {
                EntityNbt::String(_) => EntityNbt::String(target.canonical()),
                _ => EntityNbt::IntArray(target.ints()),
            },
        };
        if source != target || policy.representation != UuidRepresentation::Preserve {
            report.finding(
                "uuid.rewritten",
                Action::Redact,
                item_path,
                Some("uuid_policy"),
            );
        }
        next.push(rewritten);
    }
    *values = next;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_entity::BlockEntity;
    use crate::block_position::BlockPosition;
    use crate::Entity;

    fn sensitive_fixture() -> UniversalSchematic {
        let mut schematic = UniversalSchematic::new("sensitive".into());
        schematic.set_block(0, 0, 0, &BlockState::new("minecraft:chest"));
        let mut chest = BlockEntity::new("minecraft:chest".into(), (0, 0, 0));
        chest.nbt_mut().insert(
            "CustomName".into(),
            NbtValue::String("private chest".into()),
        );
        schematic.set_block_entity(BlockPosition { x: 0, y: 0, z: 0 }, chest);
        let mut entity = Entity::new("minecraft:armor_stand".into(), (0.5, 1.0, 0.5));
        entity
            .nbt
            .insert("UUID".into(), EntityNbt::IntArray(vec![1, 2, 3, 4]));
        entity.nbt.insert(
            "CustomName".into(),
            EntityNbt::String("private marker".into()),
        );
        schematic.add_entity(entity);
        schematic
    }

    #[test]
    fn registry_safe_is_atomic_deterministic_and_idempotent() {
        let plan = TransformPlan::registry_safe();
        let original = sensitive_fixture();
        let dry = plan.inspect(&original).unwrap();
        assert!(dry.dry_run);
        assert_eq!(original.get_entities_as_list().len(), 1);

        let mut once = original.clone();
        let applied = plan.apply(&mut once).unwrap();
        assert_eq!(dry.summary, applied.summary);
        let entity = &once.get_entities_as_list()[0];
        assert!(!entity.nbt.contains_key("CustomName"));
        assert_ne!(
            entity.nbt.get("UUID"),
            Some(&EntityNbt::IntArray(vec![1, 2, 3, 4]))
        );

        let mut twice = once.clone();
        plan.apply(&mut twice).unwrap();
        assert_eq!(
            serde_json::to_value(&once).unwrap(),
            serde_json::to_value(&twice).unwrap()
        );
    }

    #[test]
    fn transformation_artifacts_are_reproducible_across_fresh_decodes() {
        use crate::formats::schematic::{
            from_schematic, to_schematic, to_schematic_version, SchematicVersion,
        };
        let input = to_schematic(&sensitive_fixture()).unwrap();
        for plan in [TransformPlan::canonical(), TransformPlan::registry_safe()] {
            for version in [SchematicVersion::V2, SchematicVersion::V3] {
                let transform = |data: &[u8]| {
                    let mut schematic = from_schematic(data).unwrap();
                    plan.apply(&mut schematic).unwrap();
                    assert_eq!(
                        schematic
                            .metadata
                            .transformation_history
                            .last()
                            .unwrap()
                            .verification
                            .get("idempotence")
                            .map(String::as_str),
                        Some("passed")
                    );
                    to_schematic_version(&schematic, version).unwrap()
                };
                let expected = transform(&input);
                for _ in 0..16 {
                    assert_eq!(expected, transform(&input));
                }
                assert_eq!(expected, transform(&expected));
            }
        }
    }

    #[test]
    fn deterministic_uuid_paths_are_stable_after_entity_filtering() {
        let mut schematic = UniversalSchematic::new("filter-before-identity".into());
        schematic.add_entity(Entity::new("minecraft:item".into(), (0.0, 1.0, 0.0)));
        let mut stand = Entity::new("minecraft:armor_stand".into(), (1.0, 1.0, 0.0));
        stand
            .nbt
            .insert("UUID".into(), EntityNbt::IntArray(vec![1, 2, 3, 4]));
        schematic.add_entity(stand);
        let plan = TransformPlan::registry_safe();
        plan.apply(&mut schematic).unwrap();
        let once = serde_json::to_value(&schematic).unwrap();
        assert_eq!(schematic.get_entities_as_list().len(), 1);
        plan.apply(&mut schematic).unwrap();
        assert_eq!(once, serde_json::to_value(&schematic).unwrap());
    }

    #[test]
    fn entity_location_uuid_basis_survives_entity_reordering() {
        let make = |reverse: bool| {
            let mut schematic = UniversalSchematic::new("location-identities".into());
            let mut first = Entity::new("minecraft:armor_stand".into(), (1.25, 2.0, 3.5));
            first
                .nbt
                .insert("UUID".into(), EntityNbt::IntArray(vec![1, 2, 3, 4]));
            let mut second = Entity::new("minecraft:marker".into(), (8.0, 9.0, 10.0));
            second
                .nbt
                .insert("UUID".into(), EntityNbt::IntArray(vec![5, 6, 7, 8]));
            if reverse {
                schematic.add_entity(second);
                schematic.add_entity(first);
            } else {
                schematic.add_entity(first);
                schematic.add_entity(second);
            }
            schematic
        };
        let mut policy = ContentPolicy::default();
        policy.uuids.mode = UuidMode::RegenerateDeterministic;
        policy.uuids.identity_basis = UuidIdentityBasis::EntityLocation;
        policy.uuids.salt = "location-basis".into();
        let plan = TransformPlan::new(
            "location-basis",
            vec![TransformSpec::ContentPolicy { policy }],
        );
        let mut ordered = make(false);
        let mut reversed = make(true);
        plan.apply(&mut ordered).unwrap();
        plan.apply(&mut reversed).unwrap();
        let uuid_at = |schematic: &UniversalSchematic, id: &str| {
            schematic
                .get_entities_as_list()
                .into_iter()
                .find(|entity| entity.id == id)
                .and_then(|entity| entity.nbt.get("UUID").cloned())
                .unwrap()
        };
        assert_eq!(
            uuid_at(&ordered, "minecraft:armor_stand"),
            uuid_at(&reversed, "minecraft:armor_stand")
        );
        assert_eq!(
            uuid_at(&ordered, "minecraft:marker"),
            uuid_at(&reversed, "minecraft:marker")
        );
    }

    #[test]
    fn uuid_representation_can_be_standardized_without_regeneration() {
        let mut schematic = sensitive_fixture();
        let mut policy = ContentPolicy::default();
        policy.uuids.representation = UuidRepresentation::String;
        let plan = TransformPlan::new("uuid-string", vec![TransformSpec::ContentPolicy { policy }]);
        plan.apply(&mut schematic).unwrap();
        let entity = &schematic.get_entities_as_list()[0];
        assert_eq!(
            entity.nbt.get("UUID"),
            Some(&EntityNbt::String(
                "00000001-0000-0002-0000-000300000004".into()
            ))
        );
    }

    #[test]
    fn deterministic_uuid_rewrite_updates_references_together() {
        let mut schematic = sensitive_fixture();
        let source = vec![1, 2, 3, 4];
        let mut follower = Entity::new("minecraft:wolf".into(), (2.0, 1.0, 0.0));
        follower
            .nbt
            .insert("Owner".into(), EntityNbt::IntArray(source.clone()));
        schematic.add_entity(follower);

        let mut policy = ContentPolicy::default();
        policy.uuids.mode = UuidMode::RegenerateDeterministic;
        policy.uuids.representation = UuidRepresentation::IntArray;
        policy.uuids.salt = "test-namespace".into();
        let plan = TransformPlan::new(
            "rewrite-graph",
            vec![TransformSpec::ContentPolicy { policy }],
        );
        plan.apply(&mut schematic).unwrap();

        let entities = schematic.get_entities_as_list();
        let rewritten = entities[0].nbt.get("UUID").unwrap();
        assert_ne!(rewritten, &EntityNbt::IntArray(source));
        assert_eq!(entities[1].nbt.get("Owner"), Some(rewritten));
    }

    #[test]
    fn deterministic_uuid_rewrite_updates_reference_lists() {
        let mut schematic = sensitive_fixture();
        let source = EntityNbt::IntArray(vec![1, 2, 3, 4]);
        let mut fox = Entity::new("minecraft:fox".into(), (2.0, 1.0, 0.0));
        fox.nbt.insert(
            "Trusted".into(),
            EntityNbt::List(vec![source.clone(), EntityNbt::String("not-a-uuid".into())]),
        );
        schematic.add_entity(fox);

        let mut policy = ContentPolicy::default();
        policy.uuids.mode = UuidMode::RegenerateDeterministic;
        policy.uuids.representation = UuidRepresentation::String;
        policy.uuids.salt = "list-namespace".into();
        let plan = TransformPlan::new(
            "rewrite-reference-list",
            vec![TransformSpec::ContentPolicy { policy }],
        );
        plan.apply(&mut schematic).unwrap();

        let entities = schematic.get_entities_as_list();
        let rewritten = entities[0].nbt.get("UUID").unwrap();
        let EntityNbt::List(trusted) = entities[1].nbt.get("Trusted").unwrap() else {
            panic!("Trusted should remain a list");
        };
        assert_eq!(trusted.first(), Some(rewritten));
        assert_eq!(
            trusted.get(1),
            Some(&EntityNbt::String("not-a-uuid".into()))
        );
    }

    #[test]
    fn nested_leash_uuid_is_treated_as_a_reference() {
        let mut schematic = sensitive_fixture();
        let source = vec![1, 2, 3, 4];
        let mut follower = Entity::new("minecraft:wolf".into(), (2.0, 1.0, 0.0));
        follower.nbt.insert(
            "Leash".into(),
            EntityNbt::Compound(HashMap::from([(
                "UUID".into(),
                EntityNbt::IntArray(source),
            )])),
        );
        schematic.add_entity(follower);

        let mut policy = ContentPolicy::default();
        policy.uuids.mode = UuidMode::RegenerateDeterministic;
        policy.uuids.salt = "leash-namespace".into();
        let plan = TransformPlan::new(
            "rewrite-nested-leash",
            vec![TransformSpec::ContentPolicy { policy }],
        );
        plan.apply(&mut schematic).unwrap();

        let entities = schematic.get_entities_as_list();
        let rewritten = entities[0].nbt.get("UUID").unwrap();
        let EntityNbt::Compound(leash) = entities[1].nbt.get("Leash").unwrap() else {
            panic!("Leash should remain a compound");
        };
        assert_eq!(leash.get("UUID"), Some(rewritten));
    }

    #[test]
    fn uuid_assignment_and_long_pair_standardization_are_explicit() {
        let mut schematic = UniversalSchematic::new("uuid-assignment".into());
        schematic.add_entity(Entity::new("minecraft:armor_stand".into(), (0.5, 1.0, 0.5)));
        let mut policy = ContentPolicy::default();
        policy.uuids.mode = UuidMode::RegenerateDeterministic;
        policy.uuids.representation = UuidRepresentation::LongPair;
        policy.uuids.assign_missing = true;
        policy.uuids.salt = "assignment".into();
        let plan = TransformPlan::new(
            "assign-missing",
            vec![TransformSpec::ContentPolicy { policy }],
        );
        let report = plan.apply(&mut schematic).unwrap();
        let entity = &schematic.get_entities_as_list()[0];
        assert!(entity.nbt.contains_key("UUIDMost"));
        assert!(entity.nbt.contains_key("UUIDLeast"));
        assert_eq!(report.summary.get("uuid.assigned"), Some(&1));
    }

    #[test]
    fn dangling_reference_can_be_removed() {
        let mut schematic = UniversalSchematic::new("dangling".into());
        let mut wolf = Entity::new("minecraft:wolf".into(), (0.5, 1.0, 0.5));
        wolf.nbt
            .insert("Owner".into(), EntityNbt::IntArray(vec![9, 8, 7, 6]));
        schematic.add_entity(wolf);
        let mut policy = ContentPolicy::default();
        policy.uuids.dangling = DanglingReferencePolicy::Remove;
        let plan = TransformPlan::new(
            "remove-dangling",
            vec![TransformSpec::ContentPolicy { policy }],
        );
        plan.apply(&mut schematic).unwrap();
        assert!(!schematic.get_entities_as_list()[0]
            .nbt
            .contains_key("Owner"));
    }

    #[test]
    fn rejecting_policy_leaves_source_unchanged() {
        let mut schematic = sensitive_fixture();
        let before = serde_json::to_value(&schematic).unwrap();
        let mut policy = ContentPolicy::default();
        policy.entities.max_total = Some(0);
        policy.entities.excess_action = Action::Reject;
        let plan = TransformPlan::new("no-entities", vec![TransformSpec::ContentPolicy { policy }]);
        assert!(matches!(
            plan.apply(&mut schematic),
            Err(TransformError::Rejected(_))
        ));
        assert_eq!(before, serde_json::to_value(&schematic).unwrap());
    }

    #[test]
    fn block_namespace_removal_replaces_cells_and_cleans_block_entities() {
        let mut schematic = UniversalSchematic::new("block-policy".into());
        schematic.set_block(0, 0, 0, &BlockState::new("example:machine"));
        schematic.set_block_entity(
            BlockPosition { x: 0, y: 0, z: 0 },
            BlockEntity::new("minecraft:chest".into(), (0, 0, 0)),
        );
        let mut policy = ContentPolicy::default();
        policy.allowed_namespaces = Some(["minecraft".into()].into_iter().collect());
        policy.namespace_action = Action::Remove;
        let plan = TransformPlan::new(
            "minecraft-only",
            vec![TransformSpec::ContentPolicy { policy }],
        );
        let report = plan.apply(&mut schematic).unwrap();

        assert_eq!(schematic.get_block(0, 0, 0).unwrap().name, "minecraft:air");
        assert!(schematic
            .get_block_entity(BlockPosition { x: 0, y: 0, z: 0 })
            .is_none());
        assert_eq!(report.summary.get("block.cells_removed"), Some(&1));
        assert_eq!(report.summary.get("block_entity.orphan_removed"), Some(&1));
    }

    #[test]
    fn palette_canonicalization_is_idempotent() {
        let mut schematic = UniversalSchematic::new("palette".into());
        schematic.set_block(0, 0, 0, &BlockState::new("minecraft:stone"));
        schematic.set_block(1, 0, 0, &BlockState::new("minecraft:glass"));
        let plan = TransformPlan::canonical();
        plan.apply(&mut schematic).unwrap();
        assert_eq!(
            schematic.metadata.transformation_history[0]
                .verification
                .get("idempotence")
                .map(String::as_str),
            Some("passed")
        );
        let once = serde_json::to_value(&schematic).unwrap();
        plan.apply(&mut schematic).unwrap();
        assert_eq!(once, serde_json::to_value(&schematic).unwrap());
    }

    #[test]
    fn material_profiles_remap_color_families_only_with_explicit_safety() {
        let mut schematic = UniversalSchematic::new("materials".into());
        schematic.set_block(0, 0, 0, &BlockState::new("minecraft:red_wool"));
        let profile = MaterialProfile {
            name: "wool-to-concrete".into(),
            version: 1,
            target_data_version: None,
            mappings: BTreeMap::new(),
            family_mappings: vec![MaterialFamilyRule {
                source: "minecraft:{color}_wool".into(),
                target: "minecraft:{color}_concrete".into(),
                values: default_dye_colors(),
            }],
            preserve_unmentioned_properties: false,
            safety: MaterialSafety::Profile,
        };
        let plan = TransformPlan::new(
            "material-standard",
            vec![TransformSpec::RemapMaterials { profile }],
        );
        let report = plan.apply(&mut schematic).unwrap();
        assert_eq!(
            schematic.get_block(0, 0, 0).unwrap().name,
            "minecraft:red_concrete"
        );
        assert_eq!(report.summary.get("material.cells_remapped"), Some(&1));
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == "material.behavior_not_proven"));
    }

    #[test]
    fn behavior_preserving_materials_require_matching_block_roles() {
        let mut schematic = UniversalSchematic::new("safe-materials".into());
        schematic.set_block(0, 0, 0, &BlockState::new("minecraft:red_concrete"));
        schematic.set_block(1, 0, 0, &BlockState::new("minecraft:redstone_wire"));
        let profile = MaterialProfile {
            name: "verified-role-map".into(),
            version: 1,
            target_data_version: None,
            mappings: BTreeMap::from([
                (
                    "minecraft:red_concrete".into(),
                    "minecraft:blue_concrete".into(),
                ),
                (
                    "minecraft:redstone_wire".into(),
                    "minecraft:blue_concrete".into(),
                ),
            ]),
            family_mappings: Vec::new(),
            preserve_unmentioned_properties: false,
            safety: MaterialSafety::BehaviorPreserving,
        };
        let report = TransformPlan::new(
            "verified-materials",
            vec![TransformSpec::RemapMaterials { profile }],
        )
        .apply(&mut schematic)
        .unwrap();

        assert_eq!(
            schematic.get_block(0, 0, 0).unwrap().name,
            "minecraft:blue_concrete"
        );
        assert_eq!(
            schematic.get_block(1, 0, 0).unwrap().name,
            "minecraft:redstone_wire"
        );
        assert_eq!(report.summary.get("material.behavior_verified"), Some(&1));
        assert_eq!(
            report.summary.get("material.palette_mappings_skipped"),
            Some(&1)
        );
    }

    #[test]
    fn registry_safe_data_survives_sponge_and_litematic_round_trips() {
        let mut schematic = sensitive_fixture();
        TransformPlan::registry_safe()
            .apply(&mut schematic)
            .unwrap();

        let sponge = crate::formats::schematic::to_schematic(&schematic).unwrap();
        let sponge = crate::formats::schematic::from_schematic(&sponge).unwrap();
        assert_eq!(sponge.metadata.transformation_history.len(), 1);
        let sponge_be = sponge
            .get_block_entity(BlockPosition { x: 0, y: 0, z: 0 })
            .unwrap();
        assert!(!sponge_be.nbt.inner().contains_key("CustomName"));
        assert!(!sponge.get_entities_as_list()[0]
            .nbt
            .contains_key("CustomName"));

        let litematic = crate::formats::litematic::to_litematic(&schematic).unwrap();
        let litematic = crate::formats::litematic::from_litematic(&litematic).unwrap();
        assert_eq!(litematic.metadata.transformation_history.len(), 1);
        let litematic_be = litematic
            .get_block_entity(BlockPosition { x: 0, y: 0, z: 0 })
            .unwrap();
        assert!(!litematic_be.nbt.inner().contains_key("CustomName"));
        assert!(!litematic.get_entities_as_list()[0]
            .nbt
            .contains_key("CustomName"));

        let snapshot = crate::formats::snapshot::to_snapshot(&schematic).unwrap();
        let snapshot = crate::formats::snapshot::from_snapshot(&snapshot).unwrap();
        assert_eq!(snapshot.metadata.transformation_history.len(), 1);
        assert_eq!(
            snapshot.metadata.transformation_history[0].plan_name,
            "registry-safe-v1"
        );
    }
}
