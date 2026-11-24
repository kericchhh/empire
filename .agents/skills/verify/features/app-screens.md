# App screens

## How to reach it

Run `pnpm --dir app tauri dev`. `app/src/Home.tsx` owns saved-world selection and the create-world flow (scenario file or prompt, with optional agent setup). In `app/src/World.tsx`, the navigation opens Whole world, each empire, each `Seen by …` seat view, History, Make something happen, and Agents & access; the header also shows the date, turn, waiting seats, Next turn now and Close world. These commands call the Tauri backend in `app/src-tauri/src/main.rs`.

## Driving it

Open a saved world or create one from the app. In Whole world inspect empire summaries and recent events; select an empire or `Seen by …` seat to inspect its state or perception; History pages through events; Make something happen records an intervention; Agents & access shows each seat's status and lets you reveal/copy its token. The header's Next turn now force-closes a tick.

## Proof

Capture the affected screen and the corresponding world state/event through the app or `scripts/empire chronicle`. Run `pnpm --dir app exec tsc --noEmit` for type checking.

## Gotchas

App state comes from server/Tauri commands; user mutations should appear as server events. Theme terms and palette come from the scenario. Desktop launch requires the platform webview dependencies listed in `README.md`.
