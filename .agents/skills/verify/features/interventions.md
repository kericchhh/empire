# Interventions

## How to reach it

The app's world screen submits interventions through `POST /api/intervene`; the admin API is implemented in `crates/sim-server/src/http.rs`. It writes an event and can announce to every seat or only a selected empire.

## Driving it

With a running world, use its state directory (the `state_dir` field from `scripts/empire start`) and POST `{"title":"A storm","text":"Ships are delayed","effects":[],"empire":"rome"}` to `/api/intervene` with the admin bearer token:

```sh
state=/tmp/empire-control-$UID
curl -sf -X POST -H "Authorization: Bearer $(cat "$state/admin")" -H 'content-type: application/json' \
  "http://127.0.0.1:$(cat "$state/port")/api/intervene" \
  -d '{"title":"A storm","text":"Ships are delayed","effects":[],"empire":"rome"}'
```

For the desktop path, use the World screen's intervention form. Inspect resulting events through `GET /api/events?after=0&limit=500` or `scripts/empire chronicle`.

## Proof

Capture the returned sequence number and the event in `/api/events`; compare seat views to check that an empire-scoped intervention is only announced to that empire.

## Gotchas

All admin endpoints require `Authorization: Bearer <admin token>`. Keep intervention effects valid for the ruleset. Omit `empire` to announce to every seat.
