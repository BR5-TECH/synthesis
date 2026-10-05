//! What a turn is told (GRL-FR-VYNX, GXD-FR-ODGX).
//!
//! The whole of the graduation context travels in this one structured object.
//! Nothing about a run reaches an agent in the instruction, in an argument
//! vector, in an environment variable, or in a mounted file.

use super::*;

/// The version every task carries. A task at another version is refused before
/// any container is created.
pub const GRADUATION_INPUT_VERSION: u32 = 1;

pub const PURPOSE_GENERATE: &str = "generate";
pub const PURPOSE_REVISE: &str = "revise";
pub const PURPOSE_RESUME: &str = "resume";
pub const PURPOSE_ESCALATION_ANSWER: &str = "escalation_answer";

/// GRL-FR-TXEB: one unresolved path of a merge, with what each side did to it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MergeTaskConflict {
    pub path: String,
    pub base_change: String,
    pub stream_change: String,
}

/// GRL-FR-TXEB: what a turn of a merge run is told about the merge.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MergeTaskContext {
    pub stream_branch: String,
    pub base_branch: String,
    pub base_tip: String,
    pub stream_tip: String,
    pub merge_base: String,
    pub snapshot_commit: String,
    /// Every path the Git merge changes against the base tip, the Git-clean
    /// paths included.
    pub changed_paths: Vec<String>,
    /// The paths Git could not merge.
    pub unresolved_paths: Vec<String>,
    pub conflicts: Vec<MergeTaskConflict>,
    /// A merge review alone: the paths the work turns changed beyond the
    /// unresolved ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciled_paths: Option<Vec<String>>,
}

/// GRL contract surface: the input object every turn carries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GraduationTaskInput {
    pub graduation_input_version: u32,
    pub run_id: String,
    /// Which phase this turn is: `work`, `review`, or `semantic_merge`.
    pub part: String,
    pub purpose: String,
    pub draft_name: String,
    /// The captured prompt, whole.
    pub prompt: String,
    pub prompt_checksum: String,
    pub stream_name: String,
    /// The pass this turn belongs to, counted from one and bounded by the
    /// run's budget window (GXD-FR-PWYD).
    pub pass: u32,
    /// GRL-FR-GQAB: the review findings a revise pass carries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_instruction: Option<String>,
    /// What the working copy holds against the base commit.
    #[serde(default)]
    pub changed_paths: Vec<String>,
    /// What the repository's ignore rules kept out of the change set.
    #[serde(default)]
    pub hidden_paths: Vec<String>,
    /// How many more of those there were than this input carries.
    #[serde(default)]
    pub hidden_paths_omitted: u32,
    /// GRL-FR-TVXI: what the turn before this one answered wrongly, and what to
    /// send instead. Present only on a turn that is being asked again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<String>,
    /// GRL-FR-TXEB: the merge a turn of a merge run reconciles or reviews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<MergeTaskContext>,
    /// GRL-FR-DNKA: the work turn's own account of what it did and did not do,
    /// carried to the review that judges what it wrote.
    ///
    /// GRL-FR-NVBZ: a **claim** rather than a finding of fact. The review reads
    /// it and checks what it says against the change set, which is where the
    /// evidence is (GRL-FR-CKBL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_account: Option<String>,
    /// The agent's own reason, where a turn is continuing an escalation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation_context: Option<String>,
    #[serde(default)]
    pub escalation_answers: Vec<GraduationEscalationAnswer>,
}

impl GraduationTaskInput {
    /// GXD-FR-ODGX: refused before any container exists.
    /// `pass_limit` is the highest pass the run's budget window allows
    /// (GXD-FR-PWYD). Zero is no window yet, which bounds nothing.
    pub fn validate(&self, pass_limit: u32) -> Result<(), String> {
        if self.graduation_input_version != GRADUATION_INPUT_VERSION {
            return Err("graduation input version is not 1".to_string());
        }
        if self.run_id.is_empty() {
            return Err("graduation input names no run".to_string());
        }
        if self.prompt.trim().is_empty() {
            return Err("graduation input carries no prompt".to_string());
        }
        if !matches!(
            self.part.as_str(),
            phases::PART_WORK
                | phases::PART_REVIEW
                | phases::PART_SEMANTIC_MERGE
                | phases::PART_MERGE_WORK
                | phases::PART_MERGE_REVIEW
        ) {
            return Err(format!("unknown graduation part {:?}", self.part));
        }
        // GRL-FR-TXEB: the merge context travels with the merge parts and with
        // no other.
        let merge_part = matches!(
            self.part.as_str(),
            phases::PART_MERGE_WORK | phases::PART_MERGE_REVIEW
        );
        if merge_part != self.merge.is_some() {
            return Err(format!(
                "the merge context does not match the part {:?}",
                self.part
            ));
        }
        if !matches!(
            self.purpose.as_str(),
            PURPOSE_GENERATE | PURPOSE_REVISE | PURPOSE_RESUME | PURPOSE_ESCALATION_ANSWER
        ) {
            return Err(format!("unknown graduation purpose {:?}", self.purpose));
        }
        if self.pass == 0 || (pass_limit > 0 && self.pass > pass_limit) {
            return Err(format!("pass {} is outside the bound", self.pass));
        }
        Ok(())
    }

    /// Serialized for the task document.
    pub fn to_map(&self) -> serde_json::Map<String, serde_json::Value> {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        }
    }
}

/// GXD-FR-XQJR / EAC-FR-IRRD: the executor's turn kind for one part.
///
/// The kind says which of the author's own reasoning-effort selections applies
/// and which side of EAC-FR-41's masking the turn stands on.
pub(crate) fn turn_kind_for(part: &str) -> crate::tools::agent_exec::TurnKind {
    use crate::tools::agent_exec::TurnKind;
    match part {
        phases::PART_REVIEW => TurnKind::Review,
        phases::PART_SEMANTIC_MERGE => TurnKind::SemanticRebase,
        // Every turn that does the work, and anything a later part forgets to
        // name. It is never the masked kind, so a part nobody named can neither
        // reach EAC-FR-41's masking by accident nor escape it.
        _ => TurnKind::Work,
    }
}

/// GXD-FR-XQJR / EAC-FR-IRRD: the executor's turn kind for one part of one run.
///
/// A merge run's two turns have kinds of their own, which the executor resolves
/// with the author's work and review selections.
pub(crate) fn turn_kind_for_run(
    run: &GraduationRun,
    part: &str,
) -> crate::tools::agent_exec::TurnKind {
    use crate::tools::agent_exec::TurnKind;
    if run.is_merge() {
        // EAC-FR-IRRD: the review part under either of its names.
        return match part {
            phases::PART_REVIEW | phases::PART_MERGE_REVIEW => TurnKind::MergeReview,
            _ => TurnKind::MergeWork,
        };
    }
    turn_kind_for(part)
}

/// GRL-FR-TXEB: the merge a turn of a merge run is told about.
fn merge_context(run: &GraduationRun, part: &str) -> Option<MergeTaskContext> {
    let data = run.merge.as_ref()?;
    let reconciled = (part == phases::PART_REVIEW).then(|| {
        run.checkpoint
            .changed_paths
            .iter()
            .filter(|path| !data.unresolved_paths.contains(path))
            .cloned()
            .collect()
    });
    Some(MergeTaskContext {
        stream_branch: data.stream_branch.clone(),
        base_branch: data.base_branch.clone(),
        base_tip: data.base_tip.clone(),
        stream_tip: data.stream_tip.clone(),
        merge_base: data.merge_base.clone(),
        snapshot_commit: data.snapshot_commit.clone(),
        changed_paths: data.changed_paths.clone(),
        unresolved_paths: data.unresolved_paths.clone(),
        conflicts: data
            .conflicts
            .iter()
            .map(|conflict| MergeTaskConflict {
                path: conflict.path.clone(),
                base_change: conflict.base_change.clone(),
                stream_change: conflict.stream_change.clone(),
            })
            .collect(),
        reconciled_paths: reconciled,
    })
}

/// Compose the input one turn carries.
pub fn compose_input(run: &GraduationRun, part: &str, purpose: &str) -> GraduationTaskInput {
    let merge = merge_context(run, part);
    let input_part = match (run.is_merge(), part) {
        (true, phases::PART_REVIEW) => phases::PART_MERGE_REVIEW,
        (true, _) => phases::PART_MERGE_WORK,
        (false, other) => other,
    };
    GraduationTaskInput {
        merge,
        graduation_input_version: GRADUATION_INPUT_VERSION,
        run_id: run.id.clone(),
        part: input_part.to_string(),
        purpose: purpose.to_string(),
        draft_name: run.input.draft_name.clone(),
        prompt: run.input.prompt.clone(),
        prompt_checksum: run.input.prompt_checksum.clone(),
        stream_name: run.target_label(),
        pass: run.pass(),
        loop_instruction: run.checkpoint.loop_instruction.clone(),
        // Set by the caller alone, and only where a turn is asked again.
        correction: None,
        // GRL-FR-DNKA: carried to **the review that judges what it wrote**, and
        // to no other turn. A work turn is never given it: `work.md` names no
        // such field, so an account reaching one would be an input nothing
        // explains — and a turn reading its predecessor's account as if it
        // described the tree in front of it is the confusion this whole field
        // exists to prevent.
        agent_account: if part == super::phases::PART_REVIEW {
            run.checkpoint.agent_account.clone()
        } else {
            None
        },
        changed_paths: run.checkpoint.changed_paths.clone(),
        hidden_paths: run.checkpoint.hidden_paths.clone(),
        hidden_paths_omitted: run.checkpoint.hidden_paths_omitted,
        escalation_context: run.escalation.as_ref().map(|e| e.reason.clone()),
        escalation_answers: run.checkpoint.pending_escalation_answers.clone(),
    }
}
