# Connector source

**codr** ships a dedicated `wn-agent` built from the MDK revision in [`connector.toml`](../connector.toml).
The release workflow applies [`mdk-codr.patch`](mdk-codr.patch) before building it.

The patch adds founding admin selection through `group_create_with_admins` and retains the local `include_media`
history-recovery extension. Existing `group_create` clients keep creator-only defaults. Non-member admins are refused
before creation. The engine assigns the owner and creator roles in the founding state, including the initial Welcome.

The bridge probes `group_create_capabilities` before recording creation intent, so an older connector can be upgraded
without leaving a false in-flight creation marker.

The patch includes protocol and relay-backed tests. The connector archive contains the upstream license and source
provenance. The installer verifies its checksum and uses a versioned, **codr**-owned path. It does not replace a shared
`wn-agent` executable.
