// The app's Tauri commands. The shapes they take and return are generated from Rust into `bindings/`.
import { invoke } from "@tauri-apps/api/core";

import type { Chronicle } from "./bindings/Chronicle";
import type { Effect } from "./bindings/Effect";
import type { Opened } from "./bindings/Opened";
import type { OpenWorld } from "./bindings/OpenWorld";
import type { Overview } from "./bindings/Overview";
import type { Providers } from "./bindings/Providers";
import type { TurnView } from "./bindings/TurnView";

export type { Opened, OpenWorld, Overview, Providers, TurnView };
export type { Entry } from "./bindings/Entry";
export type { Provider } from "./bindings/Provider";
export type { Source } from "./bindings/Source";
export type { Target } from "./bindings/Target";

export type Id = string;

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
  intervene: (title: string, text: string, effects: Effect[], empire: Id | null) =>
    invoke<number>("intervene", { title, text, effects, empire }),
  advanceTick: () => invoke<number>("advance_tick"),
  providers: () => invoke<Providers>("providers"),
  saveClaudeToken: (token: string) => invoke<void>("save_claude_token", { token }),
  connectCodex: () => invoke<void>("connect_codex"),
  agentsAlerts: () => invoke<string[]>("agents_alerts"),
};

/** A ruleset or scenario name for an id, falling back to the id itself. */
export const label = (defs: { id: Id; name: string }[], id: Id) => defs.find((d) => d.id === id)?.name ?? id;
export const fmt = (n: number) => (Math.abs(n) >= 100 ? n.toFixed(0) : n.toFixed(1));
