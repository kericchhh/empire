# 0010 HTTP is the only transport

Accepted 2026-11-22.

## Decision

Seats connect only over MCP's streamable HTTP at `/mcp`, and the admin API is plain HTTP at `/api`. There is no stdio transport and no WebSocket endpoint.

## Why

One transport keeps a seat's token and perception path the same for local and remote agents. Nothing needs a push channel yet: the app hosts worlds in-process.

## Consequences

- A WebSocket endpoint waits for a client that needs pushed events. The broadcast it would use, `Game::subscribe`, is already built and tested.
