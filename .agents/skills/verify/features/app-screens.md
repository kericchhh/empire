# App screens

## How to reach it

`scripts/empire app --out FILE [--view TEXT]` opens a copy of the world `scripts/empire start` started in the real desktop app, under WebDriver on a headless Weston display, and saves a screenshot. Linux only: `tauri-driver` has no macOS WebDriver, so on macOS ask the user to look. One-time setup:

```sh
sudo pacman -S webkitgtk-6.0 weston        # Arch; Debian or Ubuntu: sudo apt install webkit2gtk-driver weston
cargo install --locked tauri-driver --version 2.1.0
export PATH="$HOME/.cargo/bin:$PATH"
pnpm --dir app install
```

`app/src/Home.tsx` owns saved-world selection and the create-world flow. In `app/src/World.tsx`, the navigation opens Whole world, each empire, each `Seen by …` seat view, History, Make something happen, and Agents & access; the header shows the date, turn, waiting seats, Next turn now and Close world. These call the Tauri commands in `app/src-tauri/src/main.rs`.

## Driving it

Drive the world with `scripts/empire tool` and `tick`, then capture any navigation entry by its exact text:

```sh
evidence="${TMPDIR:-/tmp}/empire-evidence/<slug>"
scripts/empire start
scripts/empire app --view 'Seen by The Senate' --out "$evidence/senate.png"
scripts/empire app --view History --out "$evidence/history.png"
```

Each call builds the app (`pnpm --dir app tauri build --debug --no-bundle`), opens the Saved tab, opens the copy (named `verify`), clicks the view, waits until nothing on the screen is loading, saves the PNG, and stops every process it started.

## Proof

Open the PNG and check the claim in it; then link it in the PR (see Evidence in `../SKILL.md`). It prints `{"status":"captured","view":…,"out":…}`. Run `pnpm --dir app exec tsc --noEmit` for type checking.

## Gotchas

- The app gets its own data directory under the state directory, so it never sees the user's saved worlds or agent logins; with no login, the rules play every seat and no subscription is spent.
- It opens a copy of the world at the moment you call it; ticks closed later in the app do not reach the CLI's world.
- `--view` takes the navigation text exactly; a wrong one fails and lists the views the app has.
- Logs from the last call stay in `<state>/app/` (`build.log`, `driver.log`, `weston.log`) until `scripts/empire stop`.
