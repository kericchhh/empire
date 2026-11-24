import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Entry, fmt, Id, label, Opened, Overview, Target, TurnView } from "./api";

type View = { kind: "world" } | { kind: "empire"; id: Id } | { kind: "seat"; id: Id } | { kind: "chronicle" } | { kind: "intervene" } | { kind: "seats" };

export function WorldView({ opened, onClose }: { opened: Opened; onClose: () => void }) {
  const [o, setO] = useState<Overview | null>(null);
  const [log, setLog] = useState<Entry[]>([]);
  const [view, setView] = useState<View>({ kind: "world" });
  const [error, setError] = useState("");
  const [alerts, setAlerts] = useState<string[]>([]);
  const last = useRef(0);
  const queue = useRef(Promise.resolve());

  // Refreshes run one after another, so two never read the chronicle from the same point.
  const refresh = useCallback(() => {
    queue.current = queue.current.then(async () => {
      try {
        setO(await api.overview());
        setAlerts(await api.agentsAlerts());
        const c = await api.chronicle(last.current);
        last.current = c.last;
        if (c.entries.length) setLog((l) => [...l, ...c.entries]);
      } catch (e) {
        setError(String(e));
      }
    });
  }, []);

  useEffect(() => {
    refresh();
    let timer = 0;
    // Events arrive in bursts when a tick closes; refresh once per burst.
    const off = listen("world-event", () => {
      clearTimeout(timer);
      timer = window.setTimeout(refresh, 150);
    });
    return () => {
      clearTimeout(timer);
      void off.then((f) => f());
    };
  }, [refresh]);

  const theme = useMemo(() => themeVars(o), [o]);
  if (!o) return <main className="loading"><p className="busy">Opening the world</p>{error && <p className="error">{error}</p>}</main>;
  const sc = o.world.scenario;
  const waiting = o.state.joined.filter((s) => !o.ended_turn.includes(s));
  const close = () => api.closeWorld().then(onClose, (e) => setError(String(e)));
  const advance = () => api.advanceTick().catch((e) => setError(String(e)));
  const is = (v: View) => JSON.stringify(v) === JSON.stringify(view);
  const nav = (v: View, text: string, sub = false) => (
    <button key={JSON.stringify(v)} className={(is(v) ? "on " : "") + (sub ? "sub" : "")} onClick={() => setView(v)}>{text}</button>
  );

  return (
    <div className="app" style={theme}>
      <header className="bar">
        <h1>{sc.name}</h1>
        <span className="date">{o.date}</span>
        <span className="tick">Turn {o.state.tick}</span>
        <span className="wait">{waiting.length > 0 ? `Waiting for: ${waiting.join(", ")}` : "Waiting for no one"}</span>
        <button className="go" onClick={advance}>Next turn now</button>
        <button onClick={close}>Close world</button>
      </header>
      <nav>
        {nav({ kind: "world" }, "Whole world")}
        {sc.empires.map((e) => [
          nav({ kind: "empire", id: e.id }, e.name),
          ...e.seats.map((s) => nav({ kind: "seat", id: s.id }, `Seen by ${s.name}`, true)),
        ])}
        {sc.game_master && nav({ kind: "seat", id: sc.game_master.id }, `Seen by ${sc.game_master.name}`)}
        <hr />
        {nav({ kind: "chronicle" }, "History")}
        {nav({ kind: "intervene" }, "Make something happen")}
        {nav({ kind: "seats" }, "Agents & access")}
      </nav>
      <main>
        {alerts.map((a) => <p key={a} className="error">{a}</p>)}
        {error && <button className="error" onClick={() => setError("")}>{error}</button>}
        {view.kind === "world" && <Empires o={o} ids={sc.empires.map((e) => e.id)} recent={o.chronicle} />}
        {view.kind === "empire" && <Empires o={o} ids={[view.id]} />}
        {view.kind === "seat" && <SeatView key={view.id} seat={view.id} seq={o.last_seq} />}
        {view.kind === "chronicle" && <Chronicle log={log} />}
        {view.kind === "intervene" && <Intervene o={o} onError={setError} />}
        {view.kind === "seats" && <Seats opened={opened} o={o} />}
      </main>
    </div>
  );
}

/** The scenario's own accent and display face, kept to the world's title band. */
function themeVars(o: Overview | null): React.CSSProperties {
  const t = o?.world.scenario.theme;
  const v: Record<string, string> = {};
  if (t?.palette[0]) v["--scn"] = t.palette[0];
  if (t?.font_display) v["--scn-font"] = `"${t.font_display}"`;
  return v as React.CSSProperties;
}

function Numbers({ rows }: { rows: { name: string; value: number }[] }) {
  return (
    <dl className="numbers">
      {rows.map((r) => <div key={r.name}><dt>{r.name}</dt><dd>{fmt(r.value)}</dd></div>)}
    </dl>
  );
}

/** The all-seeing view of some empires: their numbers, regions and people. */
function Empires({ o, ids, recent }: { o: Overview; ids: Id[]; recent?: string[] }) {
  const { scenario: sc, rules } = o.world;
  const offices = rules.offices;
  return (
    <>
      {ids.map((id) => {
        const e = sc.empires.find((x) => x.id === id)!;
        const metrics = rules.metrics.map((m) => ({ name: m.name, value: o.state.empires[id]?.metrics[m.id] ?? 0 }));
        const regions = Object.entries(o.state.regions).filter(([, r]) => r.owner === id);
        const people = Object.entries(o.state.characters).filter(([, c]) => c.empire === id);
        const def = (cid: Id) => sc.empires.flatMap((x) => x.characters).find((c) => c.id === cid);
        const region = (rid: Id) => label(sc.empires.flatMap((x) => x.regions), rid);
        return (
          <section key={id} className="card">
            <h2>{e.name}</h2>
            <p className="muted">{e.description}</p>
            <Numbers rows={metrics} />
            <h3>Regions</h3>
            <table>
              <thead><tr><th />{rules.region_stats.map((s) => <th key={s.id}>{s.name}</th>)}</tr></thead>
              <tbody>
                {regions.map(([rid, r]) => (
                  <tr key={rid}><td>{region(rid)}</td>{rules.region_stats.map((s) => <td key={s.id} className="n">{fmt(r.stats[s.id] ?? 0)}</td>)}</tr>
                ))}
              </tbody>
            </table>
            <h3>People</h3>
            <table>
              <thead><tr><th /><th>Office</th><th>Region</th>{rules.resources.map((s) => <th key={s.id}>{s.name}</th>)}</tr></thead>
              <tbody>
                {people.map(([cid, c]) => {
                  const d = def(cid);
                  return (
                    <tr key={cid} title={[d?.persona, ...(c.notes.length ? ["Notes: " + c.notes.join(" / ")] : [])].join("\n")}>
                      <td>{d?.name ?? cid}<span className="muted small"> {d?.title}{!c.alive && " · dead"}</span></td>
                      <td>{label(offices, c.office)}</td>
                      <td>{region(c.region)}</td>
                      {rules.resources.map((s) => <td key={s.id} className="n">{fmt(c.resources[s.id] ?? 0)}</td>)}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </section>
        );
      })}
      {recent && (
        <section className="card">
          <h2>Recent events</h2>
          <ul className="feed">{[...recent].reverse().map((t, i) => <li key={i}>{t}</li>)}</ul>
        </section>
      )}
    </>
  );
}

/** What one seat's characters know: exactly the perception its agent receives, reloaded as events land. */
function SeatView({ seat, seq }: { seat: Id; seq: number }) {
  const [v, setV] = useState<TurnView | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let stale = false;
    api.seatView(seat).then(
      (v) => { if (!stale) { setV(v); setError(""); } },
      (e) => { if (!stale) setError(String(e)); },
    );
    return () => { stale = true; };
  }, [seat, seq]);
  const p = v?.perception;
  if (error) return <p className="error">{error}</p>;
  if (!p) return <p className="busy">Loading</p>;
  return (
    <>
      <p className="muted">Exactly what this player sees{p.game_master ? ": the game master sees everything." : ". Numbers they could not know are left out."}</p>
      {p.you_play.map((c) => (
        <section key={c.id} className="card">
          <h2>{c.name} <span className="muted small">{c.title} · {c.office}{!c.alive && " · dead"}</span></h2>
          <p>{c.persona}</p>
          <Numbers rows={c.resources} />
          <h3>Goals</h3><ul>{c.goals.map((g) => <li key={g}>{g}</li>)}</ul>
          {c.secrets.length > 0 && <><h3>Secrets</h3><ul>{c.secrets.map((s) => <li key={s}>{s}</li>)}</ul></>}
          {c.notes.length > 0 && <><h3>Notes</h3><ul>{c.notes.map((n, i) => <li key={i}>{n}</li>)}</ul></>}
        </section>
      ))}
      {p.empires.map((e) => (
        <section key={e.id} className="card">
          <h2>{e.name}{e.id === p.your_empire && <span className="muted small"> · your empire</span>}</h2>
          {e.metrics.length ? <Numbers rows={e.metrics} /> : <p className="muted small">Its numbers are hidden from you.</p>}
        </section>
      ))}
      <section className="card">
        <h2>Regions in sight</h2>
        {p.regions.map((r) => <div key={r.id}><h3>{r.name}</h3><Numbers rows={r.stats} /></div>)}
      </section>
      <section className="card">
        <h2>Known people</h2>
        <ul>{p.people.map((x) => <li key={x.id}>{x.name} <span className="muted small">{x.title} · {x.office} · {x.empire}{!x.alive && " · dead"}</span></li>)}</ul>
      </section>
      {p.pending_rulings.length > 0 && (
        <section className="card"><h2>Waiting for the game master</h2><ul>{p.pending_rulings.map((r) => <li key={r.id}>{r.character}: {r.text}</li>)}</ul></section>
      )}
      <section className="card">
        <h2>What they saw happen</h2>
        <ul className="feed">{[...p.recent].reverse().map((t, i) => <li key={i}>{t}</li>)}</ul>
      </section>
    </>
  );
}

/** Newest first, a page at a time, so a long-running world stays quick to open and scroll. */
function Chronicle({ log }: { log: Entry[] }) {
  const [shown, setShown] = useState(200);
  return (
    <section className="card">
      <h2>History</h2>
      <ul className="feed">{log.slice(-shown).reverse().map((e) => <li key={e.seq}><span className="muted small">{e.date}</span> {e.text}</li>)}</ul>
      {log.length > shown && <button onClick={() => setShown(shown + 200)}>Show older</button>}
    </section>
  );
}

/** Inject an event by hand: announced to everyone or to one empire, with optional effects and deaths. */
function Intervene({ o, onError }: { o: Overview; onError: (e: string) => void }) {
  const { scenario: sc, rules } = o.world;
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const [empire, setEmpire] = useState("");
  const [effects, setEffects] = useState<{ target: string; delta: number }[]>([]);
  const [deaths, setDeaths] = useState<Id[]>([]);
  const [done, setDone] = useState("");
  const [sending, setSending] = useState(false);

  // Every number an intervention can move, keyed by its JSON target.
  const targets = useMemo(() => {
    const t: [string, string][] = [];
    for (const e of sc.empires) {
      for (const m of rules.metrics) t.push([JSON.stringify({ metric: { empire: e.id, metric: m.id } }), `${e.name}: ${m.name}`]);
      for (const r of e.regions) for (const s of rules.region_stats) t.push([JSON.stringify({ stat: { region: r.id, stat: s.id } }), `${r.name}: ${s.name}`]);
      for (const c of e.characters) for (const s of rules.resources) t.push([JSON.stringify({ resource: { character: c.id, resource: s.id } }), `${c.name}: ${s.name}`]);
    }
    return t;
  }, [sc, rules]);

  const submit = async () => {
    setSending(true);
    try {
      const fx = effects.map((f) => ({ target: JSON.parse(f.target) as Target, delta: f.delta }));
      await api.intervene({ title, text, effects: fx, empire: empire || null, deaths });
      setDone(`Added "${title}".`);
      setTitle(""); setText(""); setEffects([]); setDeaths([]);
    } catch (e) {
      onError(String(e));
    } finally {
      setSending(false);
    }
  };

  return (
    <section className="card">
      <h2>Make something happen</h2>
      <p className="muted">Add an event to the world. It is recorded like anything else that happens, and players learn of it on their next turn.</p>
      <label>Title<input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="A comet over the delta" /></label>
      <label>What happens<textarea rows={3} value={text} onChange={(e) => setText(e.target.value)} /></label>
      <label>Who hears about it
        <select value={empire} onChange={(e) => setEmpire(e.target.value)}>
          <option value="">Everyone</option>
          {sc.empires.map((e) => <option key={e.id} value={e.id}>{e.name} only</option>)}
        </select>
      </label>
      <h3>Changes to numbers</h3>
      {effects.map((f, i) => (
        <div key={i} className="row">
          <select value={f.target} onChange={(e) => setEffects(effects.map((x, j) => (j === i ? { ...x, target: e.target.value } : x)))}>
            {targets.map(([k, l]) => <option key={k} value={k}>{l}</option>)}
          </select>
          <input type="number" value={f.delta} onChange={(e) => setEffects(effects.map((x, j) => (j === i ? { ...x, delta: Number(e.target.value) } : x)))} />
          <button onClick={() => setEffects(effects.filter((_, j) => j !== i))}>Remove</button>
        </div>
      ))}
      <button onClick={() => setEffects([...effects, { target: targets[0][0], delta: 0 }])}>Add a change</button>
      <h3>Who dies</h3>
      <div className="row">
        {sc.empires.flatMap((e) => e.characters).filter((c) => o.state.characters[c.id]?.alive).map((c) => (
          <label key={c.id}>
            <input type="checkbox" checked={deaths.includes(c.id)} onChange={(e) => setDeaths(e.target.checked ? [...deaths, c.id] : deaths.filter((d) => d !== c.id))} />
            {c.name}
          </label>
        ))}
      </div>
      <div className="row">
        <button className="go" disabled={sending || !title.trim() || !text.trim()} onClick={submit}>Add event</button>
        {done && <span className="muted">{done}</span>}
      </div>
    </section>
  );
}

/** Who plays each seat, and how to connect an outside agent to one. */
function Seats({ opened, o }: { opened: Opened; o: Overview }) {
  const sc = o.world.scenario;
  const name = (id: Id) => sc.empires.flatMap((e) => e.seats).find((s) => s.id === id)?.name ?? sc.game_master?.name ?? id;
  return (
    <section className="card">
      <h2>Agents & access</h2>
      <p className="muted">Address for outside AI agents (MCP): <code>{opened.mcp_url}</code></p>
      <table>
        <thead><tr><th>Player</th><th>Played by</th><th>Status</th><th>Access key</th></tr></thead>
        <tbody>
          {opened.seats.map((s) => (
            <tr key={s.id}>
              <td>{name(s.id)}</td>
              <td>{s.agent ? "AI on this machine" : "Free for an outside agent; built-in rules until one joins"}</td>
              <td>{o.ended_turn.includes(s.id) ? "Done this turn" : o.state.joined.includes(s.id) ? "Thinking" : "Not joined yet"}</td>
              <td><Token value={s.token} /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

function Token({ value }: { value: string }) {
  const [state, setState] = useState<"hidden" | "shown" | "copied" | "failed">("hidden");
  if (state === "hidden") return <button onClick={() => setState("shown")}>Show</button>;
  const copy = () => navigator.clipboard.writeText(value).then(() => setState("copied"), () => setState("failed"));
  return <><code>{value}</code> <button onClick={copy}>{state === "shown" ? "Copy" : state === "copied" ? "Copied" : "Copy failed"}</button></>;
}
