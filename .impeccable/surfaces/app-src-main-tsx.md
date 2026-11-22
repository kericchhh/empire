---
version: 1
slug: "app-src-main-tsx"
primary_target: "app/src/main.tsx"
related_targets: ["app/src/Home.tsx","app/src/World.tsx","app/src/style.css"]
---

Scope: the whole desktop app (home/launcher, providers, world views). Mode: Operate.
Audience: the developer testing Empire. Task: create/open worlds, pick provider + model, watch and steer.

## Direction contract

THESIS: A dark papyrus scroll over a sunset city: the launcher unrolls like a document, the world reads like a ledger on the same paper. Refuses the soft card dashboard and the web landing page.
OWN-WORLD: Dark smoked papyrus (#241b12, baked tile app/src/paper.webp from app/scripts/paper.svg) under a calm veil; parchment ink (#f2e7c9), tan for secondary text (#b8a47e); gold (#e2b84a) for live signal and controls: faint gilt rules, brass-plate buttons, solid gold primary action. Zero radius. Departure Mono (pixel, multiples of 11px, whole-pixel positions) for chrome, labels and big numbers; JetBrains Mono 13px for running text.
STORY: The user sees a living city, knows their agent is connected, starts or reopens a world in one pass, then reads any seat's truth.
FIRST VIEWPORT: Home: full-bleed static Blender render of a sunset ancient city with harbour and farmland (app/scripts/city.py -> app/src/city.webp); centred 528px scroll between wooden rollers with gold knobs: EMPIRE wordmark, three tabs (new world / saved / providers, hotkeys 1-3), the active pane, the provider strip at the foot.
FORM: pinned by the user across the session: dark green + gold evolved into dark papyrus + gold; scene only on home; app feel, no text selection; plain-language labels.
Signature interaction: the scroll rolls to each tab's height (transform-only, compositor-driven) while the rollers' grain turns.
