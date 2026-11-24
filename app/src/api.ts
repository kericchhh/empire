// The app's Tauri commands and the shapes they return, mirroring the Rust types.
import { invoke } from "@tauri-apps/api/core";

export type Id = string;
export type Num = Record<Id, number>;

export interface Stat { id: Id; name: string; description: string }
export type Region = Stat;
export interface Character {
  id: Id; name: string; title: string; office: Id; region: Id;
  persona: string; goals: string[]; secrets: string[];
}
export interface SeatDef { id: Id; name: string; controls: Id[] }
export interface Empire {
  id: Id; name: string; description: string; culture: Record<string, string>;
  regions: Region[]; characters: Character[]; seats: SeatDef[];
}
export interface Theme { palette: string[]; font_display?: string; font_body?: string; terms: Record<string, string> }
export interface Scenario {
  name: string; description: string; theme: Theme; empires: Empire[];
  game_master?: { id: Id; name: string };
}
export interface Rules { metrics: Stat[]; region_stats: Stat[]; resources: Stat[]; offices: Stat[] }
export interface State {
  tick: number;
  empires: Record<Id, { metrics: Num }>;
  regions: Record<Id, { owner: Id; stats: Num }>;
  characters: Record<Id, { empire: Id; office: Id; region: Id; resources: Num; notes: string[]; alive: boolean }>;
  joined: Id[];
  pending: Record<string, { character: Id; text: string; declared: number }>;
}
export interface Overview {
  world: { scenario: Scenario; rules: Rules };
  state: State; date: string; last_seq: number; ended_turn: Id[]; chronicle: string[];
}

export interface Value { id: Id; name: string; value: number }
export interface Perception {
  world: { name: string; date: string; tick: number };
  seat: Id; game_master: boolean; your_empire?: Id;
  empires: { id: Id; name: string; description: string; culture: Record<string, string>; metrics: Value[] }[];
  you_play: (Character & { office_description: string; resources: Value[]; notes: string[]; alive: boolean })[];
  regions: { id: Id; name: string; owner: Id; stats: Value[] }[];
  people: { id: Id; name: string; title: string; office: string; empire: Id; alive: boolean }[];
  recent: string[];
  pending_rulings: { id: string; character: Id; text: string }[];
  actions: { id: Id; name: string; description: string }[];
}
export interface TurnView { status: "your_turn" | "waiting"; tick: number; date: string; perception?: Perception }

export interface Seat { id: Id; token: string; agent: boolean }
export interface Opened { name: string; mcp_url: string; seats: Seat[] }
export interface Entry { seq: number; date: string; text: string }
export interface Chronicle { last: number; entries: Entry[] }

export type Source = "saved" | { file: string } | { prompt: { text: string; author: string } };
export interface OpenWorld {
  name: string; source: Source; seed: number; tick_seconds: number; autoplay: boolean;
  play_all: string | null; max_concurrent: number;
}
export type Target =
  | { metric: { empire: Id; metric: Id } }
  | { stat: { region: Id; stat: Id } }
  | { resource: { character: Id; resource: Id } };
export interface Intervention { title: string; text: string; effects: { target: Target; delta: number }[]; empire: Id | null; deaths: Id[] }

/** Whether an agent CLI is installed, and whether Empire holds a login for it. */
export interface Provider { version: string | null; connected: boolean }
export interface Providers { claude: Provider; codex: Provider; sandbox_problem: string | null }
export type AgentKind = "claude" | "codex";
export const AGENTS: Record<AgentKind, string> = { claude: "Claude Code", codex: "Codex" };
/** Model choices per agent; "" leaves the choice to the CLI. */
export const MODELS: Record<AgentKind, [string, string][]> = {
  claude: [["", "CLI default"], ["haiku", "Haiku"], ["sonnet", "Sonnet"], ["opus", "Opus"]],
  codex: [["", "CLI default"]],
};
export const agent = (kind: AgentKind, model: string) => (model ? `${kind}:${model}` : kind);

export const api = {
  listWorlds: () => invoke<string[]>("list_worlds"),
  openWorld: (req: OpenWorld) => invoke<Opened>("open_world", { req }),
  closeWorld: () => invoke<void>("close_world"),
  overview: () => invoke<Overview>("overview"),
  seatView: (seat: Id) => invoke<TurnView>("seat_view", { seat }),
  chronicle: (after: number) => invoke<Chronicle>("chronicle", { after }),
  intervene: (req: Intervention) => invoke<number>("intervene", { req }),
  advanceTick: () => invoke<number>("advance_tick"),
  providers: () => invoke<Providers>("providers"),
  saveClaudeToken: (token: string) => invoke<void>("save_claude_token", { token }),
  connectCodex: () => invoke<void>("connect_codex"),
  agentsAlerts: () => invoke<string[]>("agents_alerts"),
};

/** A ruleset or scenario name for an id, falling back to the id itself. */
export const label = (defs: { id: Id; name: string }[], id: Id) => defs.find((d) => d.id === id)?.name ?? id;
export const fmt = (n: number) => (Math.abs(n) >= 100 ? n.toFixed(0) : n.toFixed(1));
