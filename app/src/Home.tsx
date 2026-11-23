import { useEffect, useRef, useState } from "react";
import { agent, AgentKind, AGENTS, api, MODELS, Opened, OpenWorld, Provider, Providers as Status, Source } from "./api";
import city from "./city.webp";

type Tab = "new" | "saved" | "providers";
type Kind = "prompt" | "file";
const initial = {
  kind: "prompt" as Kind, name: "", prompt: "", file: "", authorModel: "haiku",
  players: "claude" as AgentKind | "rules", model: "haiku", seed: 0, tick: 600, concurrent: 2,
};
type S = typeof initial;
type Setter = <K extends keyof S>(k: K) => (v: S[K]) => void;

export function Home({ onOpen }: { onOpen: (w: Opened) => void }) {
  const [tab, setTab] = useState<Tab>("new");
  const [saved, setSaved] = useState<string[]>([]);
  const [providers, setProviders] = useState<Status | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [s, setS] = useState(initial);
  const set: Setter = (k) => (v) => setS((x) => ({ ...x, [k]: v }));

  const check = () => { setProviders(null); void api.providers().then(setProviders); };
  useEffect(() => {
    check();
    void api.listWorlds().then(setSaved, (e) => setError(String(e)));
  }, []);
  const usable = (p?: Provider) => !!p?.version && p.connected && !providers?.sandbox_problem;
  const ready = (Object.keys(AGENTS) as AgentKind[]).filter((k) => usable(providers?.[k]));
  const canWrite = ready.includes("claude");
  // btop-style hotkeys: 1, 2 and 3 switch tabs unless you are typing; Escape leaves a field.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") return (document.activeElement as HTMLElement | null)?.blur();
      const t = (["new", "saved", "providers"] as Tab[])[Number(e.key) - 1];
      if (t && !(e.target as Element).closest("input, textarea")) {
        e.preventDefault();
        setTab(t);
      }
    };
    addEventListener("keydown", key);
    return () => removeEventListener("keydown", key);
  }, []);

  async function open(name: string, source: Source, message: string) {
    setBusy(message);
    setError("");
    const players = s.players !== "rules" && ready.includes(s.players) ? s.players : null;
    const req: OpenWorld = {
      name, source, seed: s.seed, tick_seconds: s.tick, autoplay: !players,
      play_all: players ? agent(players, s.model) : null, max_concurrent: s.concurrent,
    };
    try {
      onOpen(await api.openWorld(req));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy("");
    }
  }

  const canCreate = !busy && !!s.name.trim() && (s.kind === "prompt" ? !!s.prompt.trim() && canWrite : !!s.file.trim());
  const create = () =>
    open(
      s.name.trim(),
      s.kind === "prompt" ? { prompt: { text: s.prompt, author: agent("claude", s.authorModel) } } : { file: s.file },
      s.kind === "prompt" ? "Claude Code is writing the world. This takes a minute or two." : "Setting up the world.",
    );

  const tabs: [Tab, string][] = [["new", "New world"], ["saved", `Saved · ${saved.length}`], ["providers", "Providers"]];
  const { scroll, inner, height } = useRoll();
  return (
    <div className="home">
      <img className="scene" src={city} alt="" draggable={false} />
      <div className="scroll" ref={scroll} style={{ "--h": `${height}px` } as React.CSSProperties}>
        <i className="roller" />
        <div className="sheet"><div className="paper">
          <div className="sheet-inner" ref={inner}>
            <h1 className="mark">Empire</h1>
            <nav className="rail">
              {tabs.map(([t, l], i) => (
                <button key={t} className={t === tab ? "on" : ""} onClick={() => setTab(t)}><sup>{i + 1}</sup>{l}</button>
              ))}
              <i className="ink" style={{ transform: `translateX(${tabs.findIndex(([t]) => t === tab) * 100}%)` }} />
            </nav>

            <div className="panel"><div className="pane" key={tab}>
              {tab === "new" && (
                <>
                  <fieldset className="box">
                    <legend>world</legend>
                    <Field label="World name"><input value={s.name} onChange={(e) => set("name")(e.target.value)} placeholder="delta" /></Field>
                    <div className="seg">
                      {(["prompt", "file"] as Kind[]).map((k) => (
                        <button key={k} className={k === s.kind ? "on" : ""} onClick={() => set("kind")(k)}>
                          {k === "prompt" ? "Describe it" : "Load a file"}
                        </button>
                      ))}
                    </div>
                    {s.kind === "prompt" ? (
                      <>
                        <Field label="Describe the world">
                          <textarea rows={4} value={s.prompt} onChange={(e) => set("prompt")(e.target.value)}
                            placeholder="Three rival river city-states in a bronze-age delta: priest-kings, merchants, warriors" />
                        </Field>
                        <Field label="AI that writes it">
                          <Select value={s.authorModel} onChange={set("authorModel")} disabled={!canWrite}
                            options={MODELS.claude.map(([v, l]) => [v, `Claude Code · ${l}`])} />
                        </Field>
                      </>
                    ) : (
                      <Field label="Scenario file path"><input value={s.file} onChange={(e) => set("file")(e.target.value)} placeholder="/path/to/scenario.yaml" /></Field>
                    )}
                  </fieldset>
                  <Players s={s} set={set} ready={ready} />
                  <button className="go" disabled={!canCreate} onClick={create}>Create and start</button>
                  {s.kind === "prompt" && !canWrite && <p className="hint">Writing a world from a description needs Claude Code. <button className="link" onClick={() => setTab("providers")}>Connect it</button></p>}
                </>
              )}

              {tab === "saved" && (
                <>
                  {saved.length === 0 && <p className="hint">No saved worlds yet. Create one and it appears here.</p>}
                  <ul className="worlds">
                    {saved.map((w) => (
                      <li key={w}>
                        <span>{w}</span>
                        <button disabled={!!busy} onClick={() => open(w, "saved", `Reopening ${w}.`)}>Open</button>
                      </li>
                    ))}
                  </ul>
                  {saved.length > 0 && <Players s={s} set={set} ready={ready} />}
                </>
              )}

              {tab === "providers" && <Providers status={providers} recheck={check} />}
            </div></div>

            {busy && <p className="busy">{busy}</p>}
            {error && <button className="error" onClick={() => setError("")}>{error}</button>}
            <button className="strip" onClick={() => setTab("providers")}>
              <i className={providers === null ? "dot" : ready.length ? "dot on" : "dot off"} />
              {providers === null ? "Checking AI providers" : ready.length ? ready.map((k) => AGENTS[k]).join(" · ") : "No AI provider connected"}
            </button>
          </div>
        </div></div>
        <i className="roller" />
      </div>
    </div>
  );
}

/** The scroll's height follows its content, so it rolls up and unrolls as tabs change, starting
 * closed. The rollers' paper turns as the sheet winds on or off them. */
function useRoll() {
  const scroll = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(0);
  const last = useRef(0);
  useEffect(() => {
    const ro = new ResizeObserver(([e]) => {
      const h = e.borderBoxSize[0].blockSize;
      requestAnimationFrame(() => setHeight(Math.round(h)));
    });
    ro.observe(inner.current!);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    // background-position repaints the two thin rollers each frame: cheap, unlike the sheet.
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const turn = (height - last.current) / 2; // each roller takes half the change
    last.current = height;
    scroll.current!.querySelectorAll<HTMLElement>(".roller").forEach((el, i) => {
      const from = getComputedStyle(el).backgroundPositionY.split(",")[1];
      const to = `${parseFloat(from) + (i ? -turn : turn)}px`;
      el.animate([{ backgroundPositionY: `0, ${from}` }, { backgroundPositionY: `0, ${to}` }], { duration: 620, easing: "cubic-bezier(.3,.7,.2,1)" });
      el.style.backgroundPositionY = `0, ${to}`;
    });
  }, [height]);
  return { scroll, inner, height };
}

/** Who plays the seats, and how fast the world moves. */
function Players({ s, set, ready }: { s: S; set: Setter; ready: AgentKind[] }) {
  const num = (k: "seed" | "tick" | "concurrent") => (e: React.ChangeEvent<HTMLInputElement>) => set(k)(Math.max(0, Math.trunc(Number(e.target.value))) || 0);
  const players = s.players !== "rules" && ready.includes(s.players) ? s.players : null;
  const choose = (v: string) => {
    set("players")(v as S["players"]);
    set("model")("");
  };
  return (
    <fieldset className="box grid">
      <legend>agents</legend>
      <Field label="Characters played by">
        <Select value={players ?? "rules"} onChange={choose} disabled={!ready.length}
          options={[...ready.map((k): [string, string] => [k, `AI (${AGENTS[k]})`]), ["rules", "Built-in rules, no AI"]]} />
      </Field>
      <Field label="AI model">
        <Select value={s.model} onChange={set("model")} options={MODELS[players ?? "claude"]} disabled={!players} />
      </Field>
      <Field label="Agents thinking at once"><input type="number" min={1} value={s.concurrent} disabled={!players} onChange={num("concurrent")} /></Field>
      <Field label="Seconds per turn, at most"><input type="number" min={1} value={s.tick} onChange={num("tick")} /></Field>
      <Field label="Seed: same number, same world"><input type="number" min={0} step={1} value={s.seed} onChange={num("seed")} /></Field>
    </fieldset>
  );
}

function Providers({ status, recheck }: { status: Status | null; recheck: () => void }) {
  const [token, setToken] = useState("");
  const [problem, setProblem] = useState<[AgentKind, string] | null>(null);
  const [connecting, setConnecting] = useState(false);
  const state = (p?: Provider) => !status || !p ? "Checking" : !p.version ? "Not installed" : status.sandbox_problem ? "Cannot be sandboxed" : p.connected ? "Connected" : "Not connected";
  const tag = (p?: Provider) => <span className={state(p) === "Connected" ? "tag on" : "tag"}>{state(p)}</span>;
  const attempt = async (kind: AgentKind, f: () => Promise<void>) => {
    setProblem(null);
    try {
      await f();
      recheck();
    } catch (e) {
      setProblem([kind, String(e)]);
    }
  };
  const said = (kind: AgentKind) => problem?.[0] === kind && <p className="hint">{problem[1]}</p>;
  const save = () => attempt("claude", async () => {
    await api.saveClaudeToken(token);
    setToken("");
  });
  const connect = () => attempt("codex", async () => {
    setConnecting(true);
    try {
      await api.connectCodex();
    } finally {
      setConnecting(false);
    }
  });
  const claude = status?.claude;
  const codex = status?.codex;
  return (
    <>
      <p className="hint">Empire never talks to an AI service itself. Each character's AI runs as its own sealed session of an AI tool you have installed, on your own subscription, and sees none of your files, settings or account details.</p>
      <fieldset className="box">
        <legend>claude code {tag(claude)}</legend>
        {claude?.version && (
          <dl className="facts">
            <dt>Version</dt><dd>{claude.version.split(" ")[0]}</dd>
            <dt>Models</dt><dd>{MODELS.claude.slice(1).map(([, l]) => l).join(", ")}</dd>
          </dl>
        )}
        {claude && !claude.version && <p className="hint">Install Claude Code, then check again.</p>}
        {status?.sandbox_problem && <p className="hint">{status.sandbox_problem}</p>}
        {claude?.version && (
          <>
            <p className="hint">
              {claude.connected ? "To replace the login, " : "To connect it, "}run <code>claude setup-token</code> in a terminal and paste the token it prints. It uses your Claude subscription and stays in a file only you can read.
            </p>
            <Field label="Token"><input type="password" value={token} autoComplete="off" onChange={(e) => setToken(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && token.trim() && void save()} /></Field>
            <button disabled={!token.trim()} onClick={() => void save()}>Save</button>
            {said("claude")}
          </>
        )}
        <button onClick={recheck}>Check again</button>
      </fieldset>
      <fieldset className="box">
        <legend>codex {tag(codex)}</legend>
        {codex?.version && <dl className="facts"><dt>Version</dt><dd>{codex.version.split(" ").pop()}</dd></dl>}
        {codex && !codex.version && <p className="hint">Install Codex, then check again.</p>}
        {codex?.version && (
          <>
            <p className="hint">
              {codex.connected ? "To replace the login, " : "To connect it, "}log in to Codex in the browser window that opens. Empire keeps this login apart from your own Codex, uses your ChatGPT subscription, and stays in a folder only you can read.
            </p>
            <button disabled={connecting} onClick={() => void connect()}>{connecting ? "Waiting for the browser" : "Log in"}</button>
            {said("codex")}
          </>
        )}
      </fieldset>
      <fieldset className="box">
        <legend>any other ai agent <span className="tag">Manual</span></legend>
        <p className="hint">Open a world, then Agents & access gives the address an agent connects to (MCP) and each player's access key.</p>
      </fieldset>
    </>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="field"><span>{label}</span>{children}</label>;
}

function Select({ value, onChange, options, disabled }: {
  value: string; onChange: (v: string) => void; options: [string, string][]; disabled?: boolean;
}) {
  return (
    <select value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
      {options.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
    </select>
  );
}
