/**
 * cleave/types — Shared type definitions for the cleave extension.
 */

// ─── Assessment ──────────────────────────────────────────────────────────────

export interface PatternDefinition {
	name: string;
	description: string;
	keywords: string[];
	requiredAny: string[];
	expectedComponents: Record<string, string[]>;
	systemsBase: number;
	modifiersDefault: string[];
	splitStrategy: string[];
}

export interface PatternMatch {
	patternId: string;
	name: string;
	confidence: number;
	keywordsMatched: string[];
	systems: number;
	modifiers: string[];
}

export interface AssessmentResult {
	complexity: number;
	systems: number;
	modifiers: string[];
	method: "fast-path" | "heuristic";
	pattern: string | null;
	confidence: number;
	decision: "execute" | "cleave" | "needs_assessment";
	reasoning: string;
	skipInterrogation: boolean;
}

export interface AssessmentFlags {
	robust: boolean;
}

// ─── Planning ────────────────────────────────────────────────────────────────

export interface ChildPlan {
	label: string;
	description: string;
	scope: string[];
	dependsOn: string[];
	/** Spec domains this child owns (from <!-- specs: ... --> annotation). Empty if none. */
	specDomains: string[];
	/** Skill names matched to this child (from <!-- skills: ... --> annotation or auto-matched from scope). Empty if none. */
	skills: string[];
}

export interface SplitPlan {
	children: ChildPlan[];
	rationale: string;
}

// ─── Execution ───────────────────────────────────────────────────────────────

export type ChildStatus =
	| "pending"
	| "running"
	| "completed"
	| "failed"
	| "needs_decomposition";

export interface ChildState {
	childId: number;
	label: string;
	dependsOn: string[];
	status: ChildStatus;
	branch: string;
	worktreePath?: string;
	startedAt?: string;
	completedAt?: string;
	error?: string;
	/** Duration in seconds */
	durationSec?: number;
	/** "local" | "cloud" — which execution backend was used */
	backend?: "local" | "cloud";
}

export type CleavePhase =
	| "assess"
	| "plan"
	| "confirm"
	| "dispatch"
	| "harvest"
	| "reunify"
	| "report"
	| "complete"
	| "failed";

export interface CleaveState {
	runId: string;
	phase: CleavePhase;
	directive: string;
	repoPath: string;
	baseBranch: string;
	assessment: AssessmentResult | null;
	plan: SplitPlan | null;
	children: ChildState[];
	workspacePath: string;
	/** Total wall-clock duration in seconds */
	totalDurationSec: number;
	createdAt: string;
	completedAt?: string;
	error?: string;
}

// ─── Conflicts ───────────────────────────────────────────────────────────────

export type ConflictType =
	| "file_overlap"
	| "decision_contradiction"
	| "interface_mismatch"
	| "assumption_violation";

export type ConflictResolution =
	| "3way_merge"
	| "escalate_to_parent"
	| "adapter_required"
	| "verify_with_parent";

export interface Conflict {
	type: ConflictType;
	description: string;
	involved: number[];
	resolution: ConflictResolution;
}

export interface TaskResult {
	path: string;
	status: "SUCCESS" | "PARTIAL" | "FAILED" | "PENDING" | "NOT_FOUND";
	summary: string | null;
	fileClaims: string[];
	interfacesPublished: string[];
	decisions: string[];
	assumptions: string[];
}

export interface ReunificationResult {
	tasksFound: number;
	rollupStatus: "SUCCESS" | "PARTIAL" | "FAILED" | "PENDING";
	conflicts: Conflict[];
	files: string[];
	interfaces: string[];
	decisions: string[];
	readyToClose: boolean;
}

// ─── Config ──────────────────────────────────────────────────────────────────

export interface CleaveConfig {
	/** Complexity threshold — above this, the directive gets cleaved */
	threshold: number;
	/** Maximum recursion depth */
	maxDepth: number;
	/** Maximum parallel children */
	maxParallel: number;
	/** Use local model for leaf tasks when possible */
	preferLocal: boolean;
	/** Success criteria for the directive */
	successCriteria: string[];
}

export const DEFAULT_CONFIG: CleaveConfig = {
	threshold: 2.0,
	maxDepth: 3,
	maxParallel: 4,
	preferLocal: true,
	successCriteria: [],
};
