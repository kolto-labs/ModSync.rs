# P2P content identity

ModSync uses two identities because they answer different questions:

- `ContentHashSHA256` is the canonical identity of the archive bytes. It is
  required before a peer result can be accepted, and the archive is checked
  before publication.
- The BitTorrent v1 `Magnet` infohash identifies the torrent metadata and
  piece layout. It is how a client joins an existing swarm. It is not a
  substitute for the archive SHA-256.
- `TorrentInfo` is optional authored metadata for the same infohash. When
  present, it lets a client join immediately without trusting a metadata
  response from the swarm.

## Existing torrents

A guide maintainer publishes the mapping in one resource entry:
`ContentHashSHA256`, `Magnet`, and optionally `TorrentInfo`. ModSync accepts a
peer copy only after the infohash, piece metadata, archive integrity, and
authored SHA-256 all agree. `torrent-inspect` verifies that mapping locally
without opening a network session:

```text
modsync torrent-inspect archive.zip 'magnet:?xt=urn:btih:...' \
  --sha256 <authored-sha256> --torrent-info <authored-base64-info>
```

There is intentionally no unauthenticated network search from SHA-256 to
magnet. BitTorrent's infohash is not derived from the file SHA-256, and a
peer-discovered mapping would allow a swarm to authorize its own content.

## Creating torrents

`torrent-seed archive.zip` creates a new torrent for maintainer authoring and
prints its magnet, SHA-256, and (in proxied mode) `TorrentInfo`. Once recorded
in a guide, future clients use `torrent-seed --authored-magnet ...` and never
mint a second identity for the same resource. The seed path verifies the
archive first and serves a private copy; it never gives the torrent engine the
user's source file.

Peer traffic is supplementary to canonical host downloads. It is never used
to bypass login, CAPTCHA, free-user waits, or other manual host gates; when
peer transport is unavailable the canonical source remains the fallback.
