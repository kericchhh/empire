import { useEffect, useRef, useState } from "react";
import { agent, api, ClaudeStatus, MODELS, Opened, OpenWorld, Source } from "./api";
import city from "./city.webp";

type Tab = "new" | "saved" | "providers";
type Kind = "prompt" | "file";
const initial = {
  kind: "prompt" as Kind, name: "", prompt: "", file: "", authorModel: "haiku",
  players: "claude" as "claude" | "rules", model: "haiku", seed: 0, tick: 600, concurrent: 2,
};
type S = typeof initial;
type Setter = <K extends keyof S>(k: K) => (v: S[K]) => void;

export function Home({ onOpen }: { onOpen: (w: Opened) => void }) {
  const [tab, setTab] = useState<Tab>("new");
  const [saved, setSaved] = useState<string[]>([]);
  const [claude, setClaude] = useState<ClaudeStatus | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [s, setS] = useState(initial);
  const set: Setter = (k) => (v) => setS((x) => ({ ...x, [k]: v }));

  const check = () => { setClaude(null); void api.claudeStatus().then(setClaude); };
  useEffect(() => {
    check();
    void api.listWorlds().then(setSaved, (e) => setError(String(e)));
  }, []);
  const ready = !!claude?.logged_in;
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
    const agents = s.players === "claude" && ready;
    const req: OpenWorld = {
      name, source, seed: s.seed, tick_seconds: s.tick, autoplay: !agents,
      play_all: agents ? agent(s.model) : null, max_concurrent: s.concurrent,
    };
    try {
      onOpen(await api.openWorld(req));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy("");
    }
  }

  const canCreate = !busy && !!s.name.trim() && (s.kind === "prompt" ? !!s.prompt.trim() && ready : !!s.file.trim());
  const create = () =>
    open(
      s.name.trim(),
      s.kind === "prompt" ? { prompt: { text: s.prompt, author: agent(s.authorModel) } } : { file: s.file },
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
                          <Select value={s.authorModel} onChange={set("authorModel")} disabled={!ready}
                            options={MODELS.map(([v, l]) => [v, `Claude Code · ${l}`])} />
                        </Field>
                      </>
                    ) : (
                      <Field label="Scenario file path"><input value={s.file} onChange={(e) => set("file")(e.target.value)} placeholder="/path/to/scenario.yaml" /></Field>
                    )}
                  </fieldset>
                  <Players s={s} set={set} ready={ready} />
                  <button className="go" disabled={!canCreate} onClick={create}>Create and start</button>
                  {s.kind === "prompt" && !ready && <p className="hint">Writing a world from a description needs an AI provider. <button className="link" onClick={() => setTab("providers")}>Connect one</button></p>}
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

              {tab === "providers" && <Providers claude={claude} recheck={check} />}
            </div></div>

            {busy && <p className="busy">{busy}</p>}
            {error && <button className="error" onClick={() => setError("")}>{error}</button>}
            <button className="strip" onClick={() => setTab("providers")}>
              <i className={claude === null ? "dot" : ready ? "dot on" : "dot off"} />
              {claude === null ? "Checking Claude Code" : ready ? `Claude Code ${claude.version?.split(" ")[0]} · ${claude.plan ?? claude.auth_method}` : "No AI provider connected"}
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
function Players({ s, set, ready }: { s: S; set: Setter; ready: boolean }) {
  const num = (k: "seed" | "tick" | "concurrent") => (e: React.ChangeEvent<HTMLInputElement>) => set(k)(Math.max(0, Math.trunc(Number(e.target.value))) || 0);
  const agents = s.players === "claude" && ready;
  return (
    <fieldset className="box grid">
      <legend>agents</legend>
      <Field label="Characters played by">
        <Select value={ready ? s.players : "rules"} onChange={(v) => set("players")(v as S["players"])}
          options={[["claude", "AI (Claude Code)"], ["rules", "Built-in rules, no AI"]]} disabled={!ready} />
      </Field>
      <Field label="AI model">
        <Select value={s.model} onChange={set("model")} options={MODELS} disabled={!agents} />
      </Field>
      <Field label="Agents thinking at once"><input type="number" min={1} value={s.concurrent} disabled={!agents} onChange={num("concurrent")} /></Field>
      <Field label="Seconds per turn, at most"><input type="number" min={1} value={s.tick} onChange={num("tick")} /></Field>
      <Field label="Seed: same number, same world"><input type="number" min={0} step={1} value={s.seed} onChange={num("seed")} /></Field>
    </fieldset>
  );
}

function Providers({ claude, recheck }: { claude: ClaudeStatus | null; recheck: () => void }) {
  const state = claude === null ? "Checking" : !claude.version ? "Not installed" : claude.logged_in ? "Connected" : "Not logged in";
  return (
    <>
      <p className="hint">Empire never talks to an AI service itself and stores no keys. Each character's AI runs as its own sealed session of an AI tool you have installed, on your own account.</p>
      <fieldset className="box">
        <legend>claude code <span className={state === "Connected" ? "tag on" : "tag"}>{state}</span></legend>
        {claude?.version && (
          <dl className="facts">
            <dt>Version</dt><dd>{claude.version.split(" ")[0]}</dd>
            {claude.logged_in && <><dt>Login</dt><dd>{claude.auth_method}</dd><dt>Plan</dt><dd>{claude.plan ?? "n/a"}</dd></>}
            <dt>Models</dt><dd>{MODELS.slice(1).map(([, l]) => l).join(", ")}</dd>
          </dl>
        )}
        {claude && !claude.logged_in && (
          <p className="hint">
            {claude.version ? "Log in from a terminal with " : "Install Claude Code, then log in from a terminal with "}
            <code>claude auth login</code>, then check again.
          </p>
        )}
        <button onClick={recheck}>Check again</button>
      </fieldset>
      <fieldset className="box">
        <legend>codex <span className="tag">Not yet supported</span></legend>
        <p className="hint">It cannot yet be kept away from your files and from other players' secrets, so it cannot play fairly.</p>
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
