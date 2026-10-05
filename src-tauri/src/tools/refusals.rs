//! The refusals every tool in this group speaks (`TLC-tool-conventions.md`).
//!
//! A refusal is **contract rather than documentation**: it is the whole of what
//! a model has to act on when a call cannot be carried out (TLC-FR-03), so the
//! message, the normalized kind, and the retryable flag are the interface. They
//! live together in one module because the three have to agree — a sentence
//! saying a further call will not succeed beside a flag saying it might is
//! worse than either alone.
//!
//! Split from `../tools.rs` to keep that file inside the project's file-size
//! rule. Nothing here changed in the move: `tools.rs` re-exports the whole
//! module, so every `crate::tools::NAME` a tool already used still resolves.

use rig::tool::{ToolErrorKind, ToolExecutionError};

use crate::skills::Ecosystem;

// ---------------------------------------------------------------------------
// The shared refusal (TLC contract surface)
// ---------------------------------------------------------------------------

/// TLC-FR-13: the sentence every tool in this group gives a model when it is
/// asked to read a project and none is open.
///
/// It says outright that retrying will not help, because the `retryable` flag
/// alone is a hint a model is free to ignore, and a tool that refuses
/// identically forever is exactly the shape that burns an agent's turns.
pub const NO_PROJECT_OPEN: &str = "No project is open, so there is nothing to read. This will keep failing until a project is opened; do not retry.";

/// LSK-FR-09: a name that resolved to nothing.
pub const SKILL_NOT_FOUND: &str = "No skill by that name is available in this project. Call `list_skills` to see which names are, then call again with one of them.";

/// LSK-FR-15: the descriptor was there, the file behind it was not.
pub const SKILL_UNREADABLE: &str = "That skill's file could not be read. It may have changed since the list was built; call `list_skills` to see what is available now.";

/// LSK-FR-10: the opening of the ambiguity refusal, completed by the ecosystems
/// that hold the name.
pub const SKILL_AMBIGUOUS: &str =
    "More than one skill goes by that name. Call again with `ecosystem` set to one of: ";

/// LSK-FR-10, the case `ecosystem` cannot reach: two skills in the *same*
/// ecosystem declaring one name.
///
/// Told apart from [`SKILL_AMBIGUOUS`] because the advice there — narrow by
/// ecosystem — is advice the model has already taken or cannot take, and
/// repeating it is how a tool burns an agent's turns forever. Nothing here
/// blames the model: it composed a perfectly good call, and the project is
/// what is ambiguous.
pub const SKILL_NAME_NOT_UNIQUE: &str = "Two skills in the same place declare that name, so it does not identify one of them. No further call can resolve this; read the skill's file directly instead.";

/// RFT-FR-12: the model sent no path to read.
pub const PATH_BLANK: &str = "The path must name a file to read. Give it relative to the project root, for example 'src/main.ts'.";

/// RFT-FR-04: the path resolved outside the project root.
///
/// Says "this project" rather than naming a directory, because TLC-FR-09 keeps
/// absolute filesystem paths out of a message a model reads.
pub const PATH_OUTSIDE_PROJECT: &str = "That path is outside this project, which is the only place this tool can read. Call again with a path relative to the project root.";

/// RFT-FR-16: the path's final component, or an ancestor of it, is a symbolic
/// link, which the filesystem helper refuses to traverse (FSA-FR-17).
pub const PATH_THROUGH_LINK: &str = "That path reaches through a symbolic link, which this tool does not follow. Call again with the file's own path.";

/// RFT-FR-13: nothing at that path.
pub const FILE_NOT_FOUND: &str = "No file exists at that path in this project. Check the path you were given, or search again for the file you want.";

/// RFT-FR-14: a folder, told apart from a file so the model corrects the right
/// mistake. This tool enumerates nothing, so a listing is not on offer.
pub const PATH_IS_FOLDER: &str =
    "That path names a folder rather than a file. Call again with the path of a file inside it.";

/// RFT-FR-15: the bytes are not UTF-8. Not one byte of them reaches this
/// sentence, the model, or any log record.
pub const FILE_NOT_TEXT: &str = "That file is not text and cannot be returned. It may be an image, an archive, or another binary format.";

/// SPS-FR-13: what a model that sent no query is told.
pub const EMPTY_SPEC_QUERY: &str = "The query must describe the topic you need this project's specifications about. Call again with a short plain-language description of it.";

/// DST-FR-15: what a model that sent no draft query is told.
///
/// Told apart from [`EMPTY_SPEC_QUERY`] because the correction names what to
/// describe, and "planned work" is what separates this tool from the one that
/// ranks decisions already taken.
pub const EMPTY_DRAFT_QUERY: &str = "The query must describe the planned work or topic to find. Call again with a short plain-language description.";

/// NST-FR-14: what a model that sent no note query is told.
///
/// Told apart from [`EMPTY_SPEC_QUERY`] and [`EMPTY_DRAFT_QUERY`] because the
/// correction names what to describe, and what separates this tool from the
/// other two is that it ranks what the author wrote down for themselves rather
/// than what the project has decided or planned.
pub const EMPTY_NOTE_QUERY: &str = "The query must describe the topic you want this project's notes about. Call again with a short plain-language description of it.";

/// SDT-FR-GGYO: what a model that sent no document query is told.
///
/// Told apart from the other empty-query refusals because the correction names
/// what to describe, and what separates this tool from them is that it ranks the
/// reference documents the user selected rather than anything the project holds.
pub const EMPTY_DOCUMENT_QUERY: &str = "The query must describe the topic you want reference documents about. Call again with a short plain-language description of it.";

/// GDT-FR-IPJG: the model sent no document id to read.
pub const DOCUMENT_ID_BLANK: &str = "The id must name a document. Give the id exactly as a search for documents reported it.";

/// GDT-FR-MVHY: the call carried a byte range and a line range together.
pub const DOCUMENT_BOTH_RANGE_FORMS: &str = "Use either a byte range or a line range, not both. Call again with byte_offset and byte_length, or with line_offset and line_limit.";

/// GDT-FR-FZHF: the id names no document in the user's selected documents.
///
/// Retryable, and says how to recover in the same breath: a misremembered id is
/// corrected on the next call, and a model that cannot remember one at all has
/// `search_documents` to find it again.
pub const DOCUMENT_NOT_FOUND: &str = "No document with that id is in the user's selected documents. Search for documents again to get a current id.";

/// GDT-FR-IFCA: the document is in the collection and its file cannot be read.
///
/// Not retryable, and says what happened without a path: the file was moved,
/// deleted, or made unreadable, and no argument the model composes changes that.
pub const DOCUMENT_UNAVAILABLE: &str = "That document is unavailable. Its file was moved, deleted, or cannot be read.";

/// GDT-FR-HREW: the document is readable and holds no text to return, which is
/// a PDF made of images or one the extractor could not parse.
pub const DOCUMENT_NO_TEXT: &str = "That document has no text that can be extracted. It may be a scanned PDF made of images.";

/// RDT-FR-11: the model sent no draft to read.
pub const DRAFT_ID_BLANK: &str =
    "The draft ID must name a draft to read. Call again with the ID returned by `search_drafts`.";

/// RDT-FR-12: the id names no draft in the active worktree.
///
/// Retryable, and says how to recover in the same breath: a misremembered id is
/// corrected on the next call, and a model that cannot remember one at all has
/// `search_drafts` to find it again.
pub const DRAFT_NOT_FOUND: &str = "No draft exists with that ID in the open project. Check the ID or search for the draft again.";

/// RDT-FR-13: the draft's `files/` is not the single prompt DRS-FR-11 requires.
///
/// Says the condition has to be resolved elsewhere, for the reason
/// [`NO_PROJECT_OPEN`] says retrying will not help: the flag alone is a hint a
/// model may ignore, and an agent re-calling this draft forever spends its turn
/// on the one draft in the project that cannot answer. It stays retryable all
/// the same, because naming a *different* draft is an argument the model can
/// compose and the next call then succeeds (TLC-FR-11).
pub const DRAFT_INCONSISTENT: &str = "That draft does not contain its required single live prompt, so it cannot be read. It must be resolved outside this tool before a later call can succeed.";

/// RDT-FR-18: the draft is there and holds its one prompt, and that prompt could
/// not be read.
///
/// Shaped after [`SKILL_UNREADABLE`] rather than after [`DRAFT_NOT_FOUND`],
/// because what the model has to know is that its *id was right* and the
/// material is what failed. Saying "no draft exists with that ID" here would be
/// a plain falsehood about a draft the project holds, and would send the agent
/// back to `search_drafts` for an id it already had.
pub const DRAFT_PROMPT_UNREADABLE: &str = "That draft's prompt could not be read. It may not be text, or it may have changed since it was found.";

/// AUC-FR-06: the model asked the author nothing.
pub const QUESTION_BLANK: &str = "The question must say what you want to know. Write it as you would write it to a colleague.";

/// AUC-FR-10: the conversation takes no further comments (CMS-FR-17).
///
/// Says outright that a further call will not succeed, for the reason
/// [`NO_PROJECT_OPEN`] does: the `retryable` flag is a hint a model may ignore,
/// and an agent that keeps asking a locked thread spends its whole budget on a
/// question nobody will ever see.
pub const CONVERSATION_LOCKED: &str = "This conversation is locked and takes no further comments, so nothing was posted and no answer is coming. Answer with what you already have; asking again will not succeed.";

/// AUC-FR-11: the append failed for anything else.
pub const QUESTION_NOT_POSTED: &str = "The question could not be posted to this conversation.";

/// AUC-FR-QSVN: the conversation is a discussion already holding a question set
/// the author is answering.
///
/// Distinct from [`QUESTION_SET_ALREADY_PENDING`] because the consequence
/// differs: this costs the model one open question, that costs it a whole set.
pub const QUESTION_SET_PENDING_FOR_COMMENT: &str = "This discussion already has a question set waiting on the author, so nothing was posted. Answer with what you already have; asking again will not succeed.";

/// ADQ-FR-QNJU, ADQ-FR-RWTP: the discussion already holds a pending set.
///
/// Says outright that a further call will not succeed, for the reason
/// [`CONVERSATION_LOCKED`] does: no argument the model could compose frees a
/// slot the author holds, and an agent that keeps asking spends its whole budget
/// discovering that.
pub const QUESTION_SET_ALREADY_PENDING: &str = "This discussion already has a question set waiting on the author, so nothing was recorded. Answer with what you already have; asking again will not succeed.";

/// ADQ-FR-VDGT: the discussion takes no further contribution.
///
/// Distinct from [`CONVERSATION_LOCKED`] because what was lost differs: that
/// costs the model a question, this costs it the whole set it composed.
pub const QUESTION_SET_CONVERSATION_LOCKED: &str = "This conversation is locked and takes no further contribution, so nothing was recorded and no answer is coming. Answer with what you already have; asking again will not succeed.";

/// ADQ-FR-HBLN: the reservation failed for anything else.
pub const QUESTION_SET_NOT_RECORDED: &str =
    "The questions could not be recorded against this discussion.";

/// ADQ-FR-PVXK: the call carried no questions, or more than ten.
pub const WRONG_QUESTION_SET_COUNT: &str = "This call must carry between one and ten questions, and it carried a number outside that. Ask everything you are stuck on, in that many entries or fewer.";

/// ADQ-FR-PVXK: one question of the set carried no text.
pub const SET_QUESTION_BLANK: &str = "The question text must say what you want to know. Write it as you would write it to a colleague.";

/// ADQ-FR-PVXK: one question carried an option count outside two or three.
pub const WRONG_OPTION_COUNT: &str = "This question must offer two or three answers for the author to pick between, and it offered a number outside that. Where you have none to propose, ask that one question with `ask_user_comment` instead.";

/// ADQ-FR-PVXK: one option of one question carried no value.
pub const SET_OPTION_BLANK: &str =
    "An option must carry the answer it stands for. Write a short phrase the author can choose.";

/// ADQ-FR-ZBQH: one question's text is above its named limit.
pub const SET_QUESTION_TOO_LONG: &str =
    "The question is longer than 2 KiB, which is its limit. Ask one thing, more briefly.";

/// ADQ-FR-ZBQH: one option's value is above its named limit.
pub const SET_OPTION_TOO_LONG: &str =
    "An option is longer than 512 B, which is its limit. Write it as a short phrase.";

/// PDC-FR-06: the conversation is about a file already in the project, or about
/// nothing this tool can propose against.
///
/// Unreachable in practice — CVL-FR-08 attaches the tool to draft origins alone —
/// and specified anyway, because a tool's refusals are its own contract rather
/// than its caller's, and a set of tools selected elsewhere is not a guarantee
/// this tool may rest on.
pub const NOT_A_DRAFT_CONVERSATION: &str = "This conversation is not about a draft, so there is nothing here to propose a change to. Say what you would change instead; asking again will not succeed.";

/// PDC-FR-05: `path` names no file the draft holds.
///
/// Names no path of its own: TLC-FR-09 keeps a message free of anything the
/// model sent, and the advice works without it — every file of the draft was in
/// the material this agent was given, each under its draft-relative path.
pub const PROPOSAL_PATH_MISSING: &str = "This draft holds no file at that path. Use one of the draft-relative paths from the material you were given.";

/// PDC-FR-07: an empty proposal would empty the file.
pub const PROPOSAL_CONTENT_BLANK: &str =
    "Pass the file's complete new text. An empty proposal would empty the file.";

/// PDC-FR-07: the author reads the rationale beside the diff, so it has to say
/// something.
pub const PROPOSAL_RATIONALE_BLANK: &str =
    "Say why the change is worth making. The author reads this beside the diff.";

/// PDC-FR-07: the text passed is what the file already holds.
///
/// Worth its own sentence rather than a generic invalid-argument one, because the
/// correction is specific and a model that gets this back has almost always
/// echoed the file instead of rewriting it.
pub const PROPOSAL_NO_CHANGE: &str =
    "The text you passed is what the file already holds, so there is nothing to decide.";

/// PDC-FR-08: the draft's one undecided slot is taken (DCP-FR-04).
///
/// Retryable, unlike the other two `PermissionDenied` refusals here: the author's
/// decision is exactly what would make a further call succeed.
///
/// It says what to tell the author as well as what not to do, because the
/// failure mode this refusal actually produces is a model that answers it by
/// writing that it has submitted the change. That leaves the author waiting for
/// a proposal nothing recorded, while the one they have not decided is the very
/// thing holding the slot — so the sentence the model most needs is the one
/// naming the decision that is outstanding.
pub const PROPOSAL_PENDING: &str = "A change you proposed earlier is still waiting on the author's decision, and a draft holds only one undecided change at a time, so this one was not recorded. Say so plainly in your reply — tell the author their decision on your earlier proposed change is what you are waiting on — and do not describe this change as proposed, because it was not. Propose it again once they have decided.";

/// PDC-FR-09: the conversation takes no further comments, so a proposal nobody
/// can be told about is one nobody will decide.
///
/// Distinct from [`CONVERSATION_LOCKED`] because the consequence differs: a
/// question that cannot be posted costs the model a question, and a proposal that
/// cannot be posted costs it a rewrite it should now describe in prose instead.
pub const PROPOSAL_CONVERSATION_LOCKED: &str = "This conversation is locked and takes no further comments, so nothing was proposed and no decision is coming. Answer with what you already have; proposing again will not succeed.";

/// PDC-FR-11: the candidate or its comment could not be written.
pub const PROPOSAL_NOT_RECORDED: &str = "The proposed change could not be recorded.";

/// PPC-FR-07: the conversation is about a draft, a note, or nothing this tool
/// can propose against.
///
/// Unreachable in practice — CVL-FR-08 attaches the tool to artifact origins
/// alone — and specified anyway, because a tool's refusals are its own contract
/// rather than its caller's.
pub const NOT_AN_ARTIFACT_CONVERSATION: &str = "This conversation is not about a file of the project, so there is nothing here to propose a change to. Say what you would change instead; asking again will not succeed.";

/// PPC-FR-05 / PPC-FR-06: `path` names no file the project holds.
///
/// Names no path of its own: TLC-FR-09 keeps a message free of anything the
/// model sent, and the advice works without it — the file under discussion was
/// in the material this agent was given, under its project-relative path.
pub const PROMPT_PROPOSAL_PATH_MISSING: &str = "The project holds no file at that path. This tool offers a rewrite of a prompt the project already has and never creates one, so name a file that exists.";

/// PPC-FR-06: the path names a file whose resolved artifact type is not
/// `prompt`.
///
/// Told apart from [`PROMPT_PROPOSAL_PATH_MISSING`] because the corrections
/// differ: one is a path to fix, the other is a file to stop proposing against.
pub const NOT_A_PROMPT_ARTIFACT: &str = "This tool reaches a file whose type is Prompt and no other kind of file, and that path names one of another kind. Name a Prompt file instead, or say what you would change in prose.";

/// PPC-FR-08: the author reads the rationale beside the comparison, so it has to
/// say something.
///
/// Its own constant rather than [`PROPOSAL_RATIONALE_BLANK`] because the two
/// name what the author is looking at, and they are looking at different things.
pub const PROMPT_PROPOSAL_RATIONALE_BLANK: &str =
    "Say why the change is worth making. The author reads this beside the comparison.";

/// PPC-FR-09: the artifact's one undecided slot is taken (PCP-FR-04).
///
/// Retryable, unlike the other two `PermissionDenied` refusals of this tool: the
/// author's decision is exactly what would make a further call succeed.
///
/// It says what to tell the author as well as what not to do, for the reason
/// [`PROPOSAL_PENDING`] does: the failure mode this refusal actually produces is
/// a model that answers it by writing that it has submitted the change.
pub const PROMPT_PROPOSAL_PENDING: &str = "A proposed change to this file is still waiting on the author. Wait for their decision before proposing another. Say so plainly in your reply — tell the author their decision on your earlier proposed change is what you are waiting on — and do not describe this change as proposed, because it was not.";

/// RGF-FR-13: the model sent no path to read. Told apart from [`PATH_BLANK`]
/// by its example, which names a specification rather than a source file —
/// this tool reads a graduation's working copy, and the paths in it that
/// matter are the ones the change set reports.
pub const GRADUATION_PATH_BLANK: &str = "The path must name a file to read. Give it relative to the project root, for example 'specifications/ui/ABC-thing.md'.";

/// RGF-FR-06: the path resolved outside the run's execution directory.
///
/// Says "the working copy this graduation is being written in" rather than
/// "this project", because during a graduation those are routinely two
/// different trees and a model told the wrong one would go looking in the
/// wrong place (TLC-FR-09 still keeps the directory itself out of the
/// sentence).
pub const GRADUATION_PATH_OUTSIDE: &str = "That path is outside the working copy this graduation is being written in, which is the only place this tool can read. Call again with a path relative to the project root.";

/// RGF-FR-13: nothing at that path in the run's working copy.
pub const GRADUATION_FILE_NOT_FOUND: &str = "No file exists at that path in this working copy. Check the path against the change set, or read a file you know is there.";

/// RGF-FR-14: the execution directory itself is gone.
///
/// Not retryable, and says so outright for the reason [`NO_PROJECT_OPEN`]
/// does: no path the model could compose would change it, and a model that
/// kept trying would report a missing file for every path in turn.
pub const GRADUATION_WORKING_COPY_UNAVAILABLE: &str = "The working copy this graduation is being written in cannot be reached, so nothing can be read from it. This will keep failing; do not retry.";

/// ESU-FR-05: the run is not in a state that can be paused for a question.
pub const NOT_ESCALATABLE_NOW: &str =
    "This run cannot be paused for a question right now. Finish your judgement with what you have.";

/// ESU-FR-08: a question is already outstanding on this run.
pub const ALREADY_ESCALATED: &str = "A question is already waiting on the person who started this run. Finish your judgement with what you have; their answer comes back on the next attempt.";

/// A tool's refusal, in the vocabulary of `TLC-tool-conventions.md`.
///
/// Every failure any tool in this group can produce is one of these. The
/// message is what the model reads (TLC-FR-09): plain sentences naming the
/// cause and the correction, carrying no Rust type name, no backtrace, no crate
/// name, and no absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRefusal {
    /// TLC-FR-13: the tool answers from an open project and there is none.
    NoProjectOpen,
    /// TLC-FR-07: an argument the model can correct on its next call. The
    /// string is authored by the tool, never assembled out of what the model
    /// sent, so a refusal cannot echo the conversation back into a log record.
    InvalidArguments(&'static str),
    /// TLC-FR-09: the same refusal, about **one** of several arguments the model
    /// composed — the change it is about named by its place in the list.
    ///
    /// A call carrying eight changes, refused for one of them, is a call the
    /// model cannot correct without knowing which: told only that some text was
    /// not found, it can send the same eight again and no more. The two numbers
    /// are counts of what the model itself sent and the message is still fixed
    /// text this application wrote, so the refusal echoes nothing back.
    InvalidArgumentsAt {
        message: &'static str,
        /// One-based, as the model would count its own list.
        at: usize,
        of: usize,
    },
    /// LSK-FR-09: the model named a skill this project does not offer.
    SkillNotFound,
    /// LSK-FR-10: the name is held by more than one ecosystem and none was
    /// given. Carries the ecosystems rather than a rendered sentence, so the
    /// message is still assembled from a fixed set of four words and not from
    /// anything the model or the project supplied.
    SkillAmbiguous(Vec<Ecosystem>),
    /// LSK-FR-10: several skills of that name sit in one ecosystem, so the
    /// `ecosystem` argument cannot separate them and no other argument exists.
    SkillNameNotUnique,
    /// LSK-FR-15: the registry named the file and the read of it failed.
    SkillUnreadable,
    /// RFT-FR-04: the path resolved outside the project root. `PermissionDenied`
    /// rather than `InvalidArgs` because the argument was well-formed — it named
    /// somewhere this tool may not go, which is a matter of reach and not of
    /// shape.
    PathOutsideProject,
    /// RFT-FR-16: the filesystem helper refused the path for reaching through a
    /// symbolic link (FSA-FR-17).
    PathThroughLink,
    /// RFT-FR-13: nothing at that path in the project.
    FileNotFound,
    /// RFT-FR-14: a folder where a file was wanted.
    PathIsFolder,
    /// RFT-FR-15: present, readable, and not UTF-8.
    FileNotText,
    /// GDT-FR-FZHF: the id names no document of the Documents collection.
    DocumentNotFound,
    /// GDT-FR-IFCA: the document is in the collection and cannot be read now.
    DocumentUnavailable,
    /// GDT-FR-HREW: the document has no extractable text.
    DocumentNoText,
    /// AUC-FR-10: the conversation is locked, so it takes no comment from an
    /// agent any more than from a person (CMS-FR-17).
    ConversationLocked,
    /// AUC-FR-11: the append failed for a reason other than a lock.
    QuestionNotPosted,
    /// PDC-FR-06: the turn's conversation is not about a draft.
    NotADraftConversation,
    /// PDC-FR-05: the draft holds no file at the path the model named.
    ProposalPathMissing,
    /// PDC-FR-08: the draft already carries an undecided proposal (DCP-FR-04).
    ProposalPending,
    /// PDC-FR-09: the conversation takes no further comments (CMS-FR-17).
    ProposalConversationLocked,
    /// PDC-FR-11 / PPC-FR-12: the candidate or its comment could not be written.
    ProposalNotRecorded,
    /// PPC-FR-07: the turn's conversation is not about a file of the project.
    NotAnArtifactConversation,
    /// PPC-FR-05 / PPC-FR-06: the project holds no file at the path the model
    /// named.
    PromptProposalPathMissing,
    /// PPC-FR-06: the path names a file whose resolved artifact type is not
    /// `prompt`.
    NotAPromptArtifact,
    /// PPC-FR-09: the artifact already carries an undecided proposal
    /// (PCP-FR-04).
    PromptProposalPending,
    /// RDT-FR-12: the model named a draft the active worktree does not hold.
    DraftNotFound,
    /// RDT-FR-13: the draft's `files/` is not the single live prompt DRS-FR-11
    /// requires, so there is nothing to read (DRS-FR-15).
    DraftInconsistent,
    /// RDT-FR-18: the draft holds its one prompt and that prompt could not be
    /// read — it does not decode as UTF-8, or the read failed.
    DraftPromptUnreadable,
    /// RGF-FR-06: the path resolved outside the run's execution directory.
    /// `PermissionDenied` for the reason [`ToolRefusal::PathOutsideProject`]
    /// is: the argument was well-formed and named somewhere this tool may not
    /// go.
    GraduationPathOutside,
    /// RGF-FR-13: nothing at that path in the run's working copy.
    GraduationFileNotFound,
    /// RGF-FR-14: the execution directory has been removed or cannot be
    /// reached, so reading is over rather than one path being absent.
    GraduationWorkingCopyUnavailable,
    /// ESU-FR-05: the run is not one the loop can pause with a question.
    NotEscalatableNow,
    /// ESU-FR-08: this run already holds an unanswered escalation.
    AlreadyEscalated,
    /// ESU-FR-10 / ESU-FR-11: an argument the model can correct, on one named
    /// question of the set it sent.
    ///
    /// The two parts are an authored sentence and the **1-based position this
    /// application assigned**, so the message still carries nothing the model
    /// wrote (TLC-FR-09) while naming which of eight questions is at fault —
    /// without which a refusal over a set of eight is a refusal the model has to
    /// guess its way out of.
    InvalidQuestion(&'static str, usize),
    /// AUC-FR-QSVN: `ask_user_comment` was called in a discussion that already
    /// holds a question set the author is answering.
    QuestionSetPendingForComment,
    /// ADQ-FR-QNJU: the discussion already holds a pending set.
    QuestionSetAlreadyPending,
    /// ADQ-FR-VDGT: the discussion is locked.
    QuestionSetConversationLocked,
    /// ADQ-FR-HBLN: the reservation failed for any other reason.
    QuestionSetNotRecorded,
}

impl ToolRefusal {
    /// TLC-FR-10: the normalized kind, so a runtime classifies a failure
    /// without parsing its message.
    pub fn kind(&self) -> ToolErrorKind {
        match self {
            ToolRefusal::NoProjectOpen
            | ToolRefusal::SkillNotFound
            | ToolRefusal::SkillUnreadable
            | ToolRefusal::FileNotFound
            // PDC-FR-05: material the model named that the draft does not hold —
            // the same shape as a path `read_file` cannot find, and retryable for
            // the same reason (a different path reaches a file the draft has).
            | ToolRefusal::ProposalPathMissing
            // PPC-FR-05, PPC-FR-06: likewise a path naming no file of the
            // project, which is material that is not there.
            | ToolRefusal::PromptProposalPathMissing
            // RDT-FR-12: an id naming no draft is material that is not there,
            // which is what `NotFound` means (TLC-FR-10).
            | ToolRefusal::DraftNotFound
            // GDT-FR-FZHF: an id naming no selected document is material that is
            // not there, on the terms of an id naming no draft.
            | ToolRefusal::DocumentNotFound
            // RGF-FR-13, RGF-FR-14: one path that is not there, and a whole
            // working copy that is not there. Both are material that is
            // absent, which is what `NotFound` means; they differ on
            // `retryable` rather than on kind.
            | ToolRefusal::GraduationFileNotFound
            | ToolRefusal::GraduationWorkingCopyUnavailable => ToolErrorKind::NotFound,
            ToolRefusal::InvalidArguments(_)
            | ToolRefusal::InvalidArgumentsAt { .. }
            | ToolRefusal::InvalidQuestion(..)
            | ToolRefusal::SkillAmbiguous(_)
            // PPC-FR-06: the path was well-formed and names a file of the wrong
            // kind, which is an argument the model can correct.
            | ToolRefusal::NotAPromptArtifact
            | ToolRefusal::PathIsFolder => ToolErrorKind::InvalidArgs,
            // RFT-FR-04, RFT-FR-16: the path was well-formed and names somewhere
            // this tool may not reach. `PermissionDenied` says that; `NotFound`
            // would invite the model to believe the file is absent and stop.
            ToolRefusal::PathOutsideProject
            | ToolRefusal::PathThroughLink
            // RGF-FR-06: the same shape, against the run's execution directory
            // rather than against the project root.
            | ToolRefusal::GraduationPathOutside => ToolErrorKind::PermissionDenied,
            // AUC-FR-10: the arguments were well-formed and the conversation
            // simply will not take them, which is a matter of reach rather than
            // of shape — the same reason a path outside the project lands here.
            // PDC-FR-06, PDC-FR-08, PDC-FR-09: each is a well-formed call the
            // conversation or the draft will not take, which is a matter of reach
            // rather than of shape.
            // PPC-FR-07, PPC-FR-09: the same two shapes against an artifact
            // conversation and an artifact's one pending slot.
            ToolRefusal::ConversationLocked
            | ToolRefusal::NotADraftConversation
            | ToolRefusal::NotAnArtifactConversation
            | ToolRefusal::ProposalPending
            | ToolRefusal::PromptProposalPending
            // ADQ-FR-QNJU, ADQ-FR-VDGT, AUC-FR-QSVN: a slot the author holds and
            // a conversation that takes no further contribution are both matters
            // of reach rather than of shape.
            | ToolRefusal::QuestionSetPendingForComment
            | ToolRefusal::QuestionSetAlreadyPending
            | ToolRefusal::QuestionSetConversationLocked
            | ToolRefusal::ProposalConversationLocked => ToolErrorKind::PermissionDenied,
            // Neither `InvalidArgs` — no argument is at fault or would help —
            // nor `NotFound`, the skills plainly being there. TLC-FR-10's
            // "`Other` otherwise" is exactly this case. RFT-FR-15's non-text
            // file lands here for the same reason: it is present, its path was
            // correct, and no argument makes *it* readable.
            ToolRefusal::SkillNameNotUnique
            // ADQ-FR-HBLN: the write failed. No argument makes it succeed and
            // nothing is missing, which is what `Other` means (TLC-FR-10).
            | ToolRefusal::QuestionSetNotRecorded
            | ToolRefusal::FileNotText
            | ToolRefusal::QuestionNotPosted
            | ToolRefusal::ProposalNotRecorded
            // RDT-FR-13: the draft is plainly there, so not `NotFound`, and no
            // argument of this tool makes *it* readable, so not `InvalidArgs`.
            // TLC-FR-10's "`Other` otherwise" is exactly this case — the same
            // place RFT-FR-15's non-text file lands, and for the same reason.
            | ToolRefusal::DraftInconsistent
            // RDT-FR-18: likewise present, correctly named, and unreadable —
            // exactly where RFT-FR-15's non-text file lands.
            | ToolRefusal::DraftPromptUnreadable
            // ESU-FR-08, ESU-FR-05: no argument is at fault and none would
            // help, and the run is plainly there, so neither `InvalidArgs` nor
            // `NotFound` — TLC-FR-10's "`Other` otherwise" is exactly this.
            | ToolRefusal::NotEscalatableNow
            | ToolRefusal::AlreadyEscalated
            // GDT-FR-IFCA, GDT-FR-HREW: the document is plainly in the
            // collection and its id was right, so not `NotFound`, and no
            // argument makes it readable, so not `InvalidArgs`.
            | ToolRefusal::DocumentUnavailable
            | ToolRefusal::DocumentNoText => ToolErrorKind::Other,
        }
    }

    /// TLC-FR-11: whether calling again could succeed.
    ///
    /// Set explicitly rather than left to `ToolErrorKind`'s own default, which
    /// calls `InvalidArgs` unretryable — the opposite of what this group means
    /// by it. The test is whether any argument the model could compose would
    /// change the answer, which makes the flag independent of the kind: a
    /// `NotFound` naming material the model chose is retryable, because a
    /// different name reaches a skill this project does have, while the one
    /// `NotFound` no argument touches — no project open — is not.
    pub fn retryable(&self) -> bool {
        match self {
            // Neither is reachable by any argument: one wants a project the
            // model cannot open, the other a name the project made ambiguous.
            // AUC-FR-10 joins them: no argument a model composes unlocks a
            // conversation, so a further call refuses identically.
            ToolRefusal::NoProjectOpen
            | ToolRefusal::SkillNameNotUnique
            | ToolRefusal::ConversationLocked
            // PDC-FR-06, PDC-FR-09: no argument makes an artifact into a draft or
            // unlocks a conversation, and an agent told otherwise would spend its
            // remaining calls proposing somewhere that cannot hold a proposal.
            | ToolRefusal::NotADraftConversation
            // PPC-FR-07: no argument makes a draft or a note into a file of the
            // project, on exactly those terms.
            | ToolRefusal::NotAnArtifactConversation
            | ToolRefusal::ProposalConversationLocked
            // RGF-FR-14: no path the model composes brings a removed working
            // copy back. ESU-FR-08, ESU-FR-05: no argument un-escalates a run
            // or makes a discarded one pausable, and a model told otherwise
            // would spend its remaining calls asking.
            | ToolRefusal::GraduationWorkingCopyUnavailable
            | ToolRefusal::NotEscalatableNow
            // ADQ-FR-RWTP, AUC-FR-QSVN: no argument the model composes frees a
            // slot the author holds, and no call reaches a locked conversation.
            | ToolRefusal::QuestionSetPendingForComment
            | ToolRefusal::QuestionSetAlreadyPending
            | ToolRefusal::QuestionSetConversationLocked
            | ToolRefusal::AlreadyEscalated
            // GDT-FR-IFCA, GDT-FR-HREW: no id the model composes brings a moved
            // file back or puts text into a scan.
            | ToolRefusal::DocumentUnavailable
            | ToolRefusal::DocumentNoText => false,
            // Every one of these turns on a value the model chose, so a
            // different call reaches something real: another name, another path,
            // a text file instead of a binary one.
            ToolRefusal::InvalidArguments(_)
            | ToolRefusal::InvalidArgumentsAt { .. }
            | ToolRefusal::InvalidQuestion(..)
            | ToolRefusal::SkillNotFound
            | ToolRefusal::SkillAmbiguous(_)
            | ToolRefusal::SkillUnreadable
            | ToolRefusal::PathOutsideProject
            | ToolRefusal::PathThroughLink
            | ToolRefusal::FileNotFound
            | ToolRefusal::PathIsFolder
            | ToolRefusal::FileNotText
            // AUC-FR-11: a write that failed once may well succeed next time.
            | ToolRefusal::QuestionNotPosted
            // ADQ-FR-HBLN: likewise a reservation.
            | ToolRefusal::QuestionSetNotRecorded
            | ToolRefusal::ProposalPathMissing
            | ToolRefusal::ProposalNotRecorded
            // PPC-FR-05, PPC-FR-06: both turn on the path the model chose, so a
            // different one reaches a prompt the project holds.
            | ToolRefusal::PromptProposalPathMissing
            | ToolRefusal::NotAPromptArtifact
            // PDC-FR-08: the author deciding the pending proposal is exactly what
            // makes a further call succeed, which is what `retryable` asks.
            | ToolRefusal::ProposalPending
            // PPC-FR-09: on exactly those terms, against an artifact.
            | ToolRefusal::PromptProposalPending
            // RDT-FR-12, RDT-FR-13: both turn on which draft the model named, so
            // a different id reaches one the worktree holds and can read. The
            // inconsistent one is retryable on that reading alone — its own
            // message says the draft itself will not become readable by calling
            // again, so the model is not invited to keep asking for this one.
            | ToolRefusal::DraftNotFound
            // GDT-FR-FZHF: a different id reaches a document the collection holds.
            | ToolRefusal::DocumentNotFound
            | ToolRefusal::DraftInconsistent
            // RDT-FR-18: a read that failed once may well succeed next time, on
            // the same terms AUC-FR-11's failed append is retryable.
            | ToolRefusal::DraftPromptUnreadable
            // RGF-FR-06, RGF-FR-13: both turn on the path the model chose, so a
            // different one reaches a file the working copy holds.
            | ToolRefusal::GraduationPathOutside
            | ToolRefusal::GraduationFileNotFound => true,
        }
    }

    /// A stable code for the log record (TLC-FR-14).
    ///
    /// Logged instead of the message so a record stays constant down a column
    /// and searchable on a term the emit site chose.
    pub fn reason(&self) -> &'static str {
        match self {
            ToolRefusal::NoProjectOpen => "no_project_open",
            ToolRefusal::InvalidArguments(_)
            | ToolRefusal::InvalidArgumentsAt { .. } => "invalid_arguments",
            ToolRefusal::InvalidQuestion(..) => "invalid_arguments",
            ToolRefusal::SkillNotFound => "skill_not_found",
            ToolRefusal::SkillAmbiguous(_) => "skill_ambiguous",
            ToolRefusal::SkillNameNotUnique => "skill_name_not_unique",
            ToolRefusal::SkillUnreadable => "skill_unreadable",
            ToolRefusal::PathOutsideProject => "path_outside_project",
            ToolRefusal::PathThroughLink => "path_through_link",
            ToolRefusal::FileNotFound => "file_not_found",
            ToolRefusal::PathIsFolder => "path_is_folder",
            ToolRefusal::FileNotText => "file_not_text",
            ToolRefusal::ConversationLocked => "conversation_locked",
            ToolRefusal::QuestionSetPendingForComment => "question_set_pending",
            ToolRefusal::QuestionSetAlreadyPending => "question_set_already_pending",
            ToolRefusal::QuestionSetConversationLocked => "conversation_locked",
            ToolRefusal::QuestionSetNotRecorded => "question_set_not_recorded",
            ToolRefusal::QuestionNotPosted => "question_not_posted",
            ToolRefusal::NotADraftConversation => "not_a_draft_conversation",
            ToolRefusal::ProposalPathMissing => "proposal_path_missing",
            ToolRefusal::ProposalPending => "proposal_pending",
            ToolRefusal::ProposalConversationLocked => "proposal_conversation_locked",
            ToolRefusal::ProposalNotRecorded => "proposal_not_recorded",
            ToolRefusal::NotAnArtifactConversation => "not_an_artifact_conversation",
            ToolRefusal::PromptProposalPathMissing => "artifact_not_found",
            ToolRefusal::NotAPromptArtifact => "not_a_prompt_artifact",
            ToolRefusal::PromptProposalPending => "proposal_pending",
            ToolRefusal::DraftNotFound => "draft_not_found",
            ToolRefusal::DocumentNotFound => "document_not_found",
            ToolRefusal::DocumentUnavailable => "document_unavailable",
            ToolRefusal::DocumentNoText => "document_no_text",
            ToolRefusal::DraftInconsistent => "draft_not_single_file",
            ToolRefusal::DraftPromptUnreadable => "draft_prompt_unreadable",
            ToolRefusal::GraduationPathOutside => "path_outside_working_copy",
            ToolRefusal::GraduationFileNotFound => "file_not_found",
            ToolRefusal::GraduationWorkingCopyUnavailable => "working_copy_unavailable",
            ToolRefusal::NotEscalatableNow => "not_escalatable_now",
            ToolRefusal::AlreadyEscalated => "already_escalated",
        }
    }

    /// The refusal as `rig` delivers it to the model (TLC-FR-10, TLC-FR-11).
    pub fn to_execution_error(&self) -> ToolExecutionError {
        ToolExecutionError::new(self.kind(), self.to_string()).with_retryable(self.retryable())
    }
}

impl std::fmt::Display for ToolRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolRefusal::NoProjectOpen => f.write_str(NO_PROJECT_OPEN),
            ToolRefusal::InvalidArguments(message) => f.write_str(message),
            ToolRefusal::InvalidArgumentsAt { message, at, of } => {
                write!(f, "Change {at} of {of}: {message}")
            }
            // The number is the position this application assigned, in the order
            // the model wrote its questions — not a value it supplied.
            ToolRefusal::InvalidQuestion(message, position) => {
                write!(f, "{message} This is question {position} of the set you sent.")
            }
            ToolRefusal::SkillNotFound => f.write_str(SKILL_NOT_FOUND),
            ToolRefusal::SkillNameNotUnique => f.write_str(SKILL_NAME_NOT_UNIQUE),
            ToolRefusal::SkillUnreadable => f.write_str(SKILL_UNREADABLE),
            ToolRefusal::PathOutsideProject => f.write_str(PATH_OUTSIDE_PROJECT),
            ToolRefusal::PathThroughLink => f.write_str(PATH_THROUGH_LINK),
            ToolRefusal::FileNotFound => f.write_str(FILE_NOT_FOUND),
            ToolRefusal::PathIsFolder => f.write_str(PATH_IS_FOLDER),
            ToolRefusal::FileNotText => f.write_str(FILE_NOT_TEXT),
            ToolRefusal::ConversationLocked => f.write_str(CONVERSATION_LOCKED),
            ToolRefusal::QuestionNotPosted => f.write_str(QUESTION_NOT_POSTED),
            ToolRefusal::QuestionSetPendingForComment => {
                f.write_str(QUESTION_SET_PENDING_FOR_COMMENT)
            }
            ToolRefusal::QuestionSetAlreadyPending => f.write_str(QUESTION_SET_ALREADY_PENDING),
            ToolRefusal::QuestionSetConversationLocked => {
                f.write_str(QUESTION_SET_CONVERSATION_LOCKED)
            }
            ToolRefusal::QuestionSetNotRecorded => f.write_str(QUESTION_SET_NOT_RECORDED),
            ToolRefusal::NotADraftConversation => f.write_str(NOT_A_DRAFT_CONVERSATION),
            ToolRefusal::ProposalPathMissing => f.write_str(PROPOSAL_PATH_MISSING),
            ToolRefusal::ProposalPending => f.write_str(PROPOSAL_PENDING),
            ToolRefusal::ProposalConversationLocked => f.write_str(PROPOSAL_CONVERSATION_LOCKED),
            ToolRefusal::ProposalNotRecorded => f.write_str(PROPOSAL_NOT_RECORDED),
            ToolRefusal::NotAnArtifactConversation => f.write_str(NOT_AN_ARTIFACT_CONVERSATION),
            ToolRefusal::PromptProposalPathMissing => f.write_str(PROMPT_PROPOSAL_PATH_MISSING),
            ToolRefusal::NotAPromptArtifact => f.write_str(NOT_A_PROMPT_ARTIFACT),
            ToolRefusal::PromptProposalPending => f.write_str(PROMPT_PROPOSAL_PENDING),
            ToolRefusal::DraftNotFound => f.write_str(DRAFT_NOT_FOUND),
            ToolRefusal::DocumentNotFound => f.write_str(DOCUMENT_NOT_FOUND),
            ToolRefusal::DocumentUnavailable => f.write_str(DOCUMENT_UNAVAILABLE),
            ToolRefusal::DocumentNoText => f.write_str(DOCUMENT_NO_TEXT),
            ToolRefusal::DraftInconsistent => f.write_str(DRAFT_INCONSISTENT),
            ToolRefusal::DraftPromptUnreadable => f.write_str(DRAFT_PROMPT_UNREADABLE),
            ToolRefusal::GraduationPathOutside => f.write_str(GRADUATION_PATH_OUTSIDE),
            ToolRefusal::GraduationFileNotFound => f.write_str(GRADUATION_FILE_NOT_FOUND),
            ToolRefusal::GraduationWorkingCopyUnavailable => {
                f.write_str(GRADUATION_WORKING_COPY_UNAVAILABLE)
            }
            ToolRefusal::NotEscalatableNow => f.write_str(NOT_ESCALATABLE_NOW),
            ToolRefusal::AlreadyEscalated => f.write_str(ALREADY_ESCALATED),
            // Every word after the fixed opening comes from `Ecosystem`, whose
            // four spellings are compiled in — so this sentence carries no path,
            // no skill name, and nothing the model sent (TLC-FR-09).
            ToolRefusal::SkillAmbiguous(ecosystems) => {
                f.write_str(SKILL_AMBIGUOUS)?;
                for (i, ecosystem) in ecosystems.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(ecosystem.as_str())?;
                }
                f.write_str(".")
            }
        }
    }
}

impl std::error::Error for ToolRefusal {}
