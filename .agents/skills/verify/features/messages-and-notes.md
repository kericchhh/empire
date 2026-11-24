# Messages and notes

## How to reach it

Start a world with at least two character seats. `message` and `note` are MCP tools in `crates/sim-server/src/mcp.rs`; messages are delivered immediately to the addressed character, while notes are private memory for the sending character.

## Driving it

Call `scripts/empire tool <seat> message '{"from":"<character>","to":"<character>","text":"<message>"}'` to send a private message. Call `scripts/empire tool <seat> note '{"character":"<character>","text":"<private note>"}'` to save a note. Refresh the recipient or sender with `look`.

## Proof

Save the successful tool response and the recipient's `look`, which should contain the quoted incoming message. A later `look` for the note's character should contain the private note.

## Gotchas

Use character ids from perception. Only the recipient's seat and game master see a private message. Notes do not send a message to another seat.
