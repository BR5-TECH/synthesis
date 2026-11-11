// The application's shared types (the frontend half of every backend
// contract). One file per area; this barrel is what every import site
// names, so `from "../types"` resolves here exactly as it did when this
// was a single file.

export * from "./core";
export * from "./flowPlan";
export * from "./changes";
export * from "./diff";
export * from "./search";
export * from "./embeddings";
export * from "./drafts";
export * from "./newDraftProposals";
export * from "./newChangeProposals";
export * from "./changeHistory";
export * from "./publication";
export * from "./statistics";
export * from "./graduation";
export * from "./graduationLogs";
export * from "./streams";
export * from "./voice";
export * from "./agentActivity";
export * from "./seeds";
export * from "./progress";
export * from "./projectConfig";
export * from "./openspec";
export * from "./changeRuns";
export * from "./changeProposals";
export * from "./logging";
export * from "./worktree";
export * from "./gitHistory";
export * from "./github";
export * from "./agentic";
export * from "./aiApi";
export * from "./comments";
export * from "./agents";
export * from "./agentConversations";
export * from "./docker";
export * from "./isolation";
export * from "./remoteConnectivity";
export * from "./githubPolling";
export * from "./documents";
export * from "./documentExport";
export * from "./modelDowngrades";
export * from "./mainBranchWrite";
