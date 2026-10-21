# M20 Hosting safety

Self-hosted servers run places and players their host doesn't control.

1. Script limits on servers: memory, run time per frame, instance counts,
   with clear errors instead of a hung server.
2. Server authority: what a client may change, as Roblox's
   FilteringEnabled rules allow, tested against a hostile client.
3. HttpService and other outward features off unless the host turns
   them on.
4. Logs and crash reports a host can act on.
