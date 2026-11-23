# NetheriteParser

A subscription builder with two independent judges and one policy: publish only
what a real client can actually reach, at a speed a person can use.

It fetches a list of public subscription sources, parses every link it
understands, removes duplicates by the strict identity (transport, security,
SNI, path — not just `address:port`), rejects blacklisted addresses, and then
runs each remaining node through **both** a real Xray process and a real mihomo
process, Xray first. A node is published only if that core gets an exit address
**and** a download clears the speed floor (15 Mbit/s by default, adapted down only when the
local line itself is slower).

The two sets are deliberately separate:

* `sub/default` — nodes that passed the **Xray** probe, for Xray-family clients;
* `sub/mihomo` — nodes that passed the **mihomo** probe, for Clash/Meta
  clients.

They can differ. A config that Xray accepts and runs can still be rejected by
mihomo's parser or transport handling, and the opposite also happens. One
concrete case: mihomo refuses Shadowsocks `chacha20-poly1305` as an unknown
method, while Xray runs it; such a node belongs in `sub/default` only.
`meta.json` reports both protocol breakdowns instead of pretending one core
implies the other.

## Requirements

* Rust 1.75+
* `mihomo` on `PATH` (Mihomo Meta v1.18+; tested with v1.19.26)
* `xray` on `PATH` (Xray-core; tested with 26.3.27)
* `v2keylib` at `../v2keylib` (path dependency)

## Usage

```sh
cargo build --release
./target/release/netherite                     # full run with sources.txt
./target/release/netherite --dry-run           # fetch and merge only
./target/release/netherite --max-sources 6 --max-nodes 60   # a quick trial
./target/release/netherite --threshold 50 --concurrency 8
./target/release/netherite --config other.toml # a different settings file
```

`configuration.toml` holds the limits (`max_nodes`, `concurrency`,
`retry_dead`), the thresholds (`speed_mbps`, timeouts, `speed_bytes`) and the
identity (`name`, `announce`). A flag on the command line overrides the file,
and the file overrides the built-in defaults, so a run is tuned in one place and
an experiment is still a single flag. An unknown key in the file is an error
rather than a silently ignored line.

Run `netherite --help` for every option.

A full run costs two probes per candidate, one through each core, so it is
measured in hours rather than minutes: every candidate costs a mihomo process, a
handshake and an exit lookup, then the same through Xray, and both downloads are
serialized on the shared line. Start with `--max-nodes`, then remove the cap.

Public sources are flaky, so a node that does not answer can be probed again
before it is written to `dead.txt` with `--retry-dead 1`. A second attempt costs
a handshake, not a download, and rescues the nodes that lost a race on the first
try; it is off by default because it roughly doubles the time spent on nodes
that are dead for good.

On a terminal the run draws one line: a spinner, a progress bar, the
published/slow/dead counters, an ETA and the node that just finished. When
stderr is a pipe or a file it prints a line every five percent instead, so a
build log is not a thousand nearly identical lines.

## Output

`sub/default` — link subscription of the nodes that passed **Xray**, VLESS first:

```
#profile-title: base64:8J+qqCBOZXRoZXJpdGU=
#profile-update-interval: 1
#subscription-userinfo: upload=0; download=0; total=0; expire=0
#announce: 🪨 Netherite — verified through mihomo and Xray
vless://...
```

The first three lines are written exactly; the title is base64 for `🪨 <name>`.
The `#announce` line is written only when `identity.announce` is non-empty, and
the title and every node name follow `identity.name`.

`sub/mihomo` — mihomo / Clash.Meta YAML with a `PROXY` select group. Every node
in it passed the **mihomo** probe and the speed floor. Validate with:

```sh
mihomo -d /tmp/check -f sub/mihomo -t
```

The two files are not copies of each other. A node that answers only through
Xray is in `sub/default` and not in `sub/mihomo`; one that answers only through
mihomo is in `sub/mihomo` and not in `sub/default`.

`sub/meta.json` — versions and counts:

| field | meaning |
| --- | --- |
| `mihomo_ver`, `xray_ver` | binaries found for this run |
| `total_cfg` | link-shaped lines in all bodies |
| `total_parsed_cfg` | nodes the parser understood |
| `total_merged_cfg` | candidates after dedup and `--max-nodes` |
| `total_dead_cfg` | merged minus published in either subscription (dead, slow, blacklisted) |
| `total_xray_cfg` | nodes in `sub/default` |
| `total_mihomo_cfg` | nodes in `sub/mihomo` |
| `total_vless_cfg`, `total_ss_cfg`, `total_trojan_cfg`, `total_vmess_cfg`, `total_hy2_cfg` | protocol split of the mihomo set (same numbers as `mihomo_compatible`) |
| `mihomo_compatible` | `{ "vless": N, "vmess": N, "trojan": N, "ss": N, "hy2": N }` for `sub/mihomo` |
| `xray_compatible` | the same object for `sub/default` |

`mihomo_compatible` and `xray_compatible` always carry the five protocol keys,
even when a count is zero, so a reader can index one without checking for its
absence. An `other` key appears only when that category is non-zero.

`sub/dead.txt` — rejected links, one per line; feed it back with `--resume`.

`sub/source_report.txt` — per-source raw/parsed/unique/alive/passed counts and
the ranked list of sources that actually produced published nodes.

## Node names

```
<flag> <English country name> - <identity.name> - <four digits>
```

The country comes from the **measured exit**, not from the source's label. The
four digits are random per run and unique within it; `--seed N` makes them
reproducible.

## Blacklist

`blacklist.txt` accepts one IP or CIDR per line. Both the entry address (DNS
answers included) and the measured exit address are checked. A hit is written
to `sub/dead.txt` and never published. The file is optional; without it every
address is eligible and the run says so.

There is no allowlist or whitelist. Filtering is blacklist-only: a parsed config
is eligible unless its entry address or measured exit address is on the list.

## Sources

`sources.txt` is a plain list of `name URL` lines. Order is priority:
`--max-sources` takes the first N, and merging interleaves in that order.

LTE/mobile-carrier and carrier-whitelist bypass exports, pozor/lexavip and the
AI/Gemini exports are deliberately absent. Dead sources were removed rather than
left in to fail on every run.

## Layout

```
src/
  cli.rs, settings.rs        arguments and defaults
  pipeline.rs                the order of a run
  sources/                   registry, fetch, raw counting, report
  blacklist/                 list, DNS resolution, entry/exit matching
  collect.rs, dedup.rs       merge, provenance, strict dedup
  geo/                       ISO names, flags, published name
  check/                     mihomo and Xray probes, speed floor, verdicts, batch
  output/                    headers, links, mihomo, meta, dead cache
  progress.rs, report.rs     stderr progress and stdout summary
  models.rs, util.rs         shared data and small helpers
```

The unit tests cover the parts that are easy to get subtly wrong: header bytes,
name shape, four-digit uniqueness, blacklist parsing, dedup attribution, caps
and `meta.json` fields. Run them with `cargo test`.

## Notes on the probes

* Every check uses a fresh process with a one-proxy config, for both cores;
  nothing is shared between nodes, so "which node answered" is never ambiguous.
* Xray and mihomo are both measured, Xray first and then mihomo, against the
  same speed floor.
  A config is not assumed to be portable between them: the difference is the
  reason `sub/default` and `sub/mihomo` exist as separate sets.
* Exit lookups race several services: ip-api (address, country, ISP), then
  country-aware HTTPS fallbacks, then bare-address fallbacks. A node that
  blocks plain HTTP is still measured.
* Speed is taken after the exit answer, with its own timeout. A slow node is
  rejected as too slow, not reported as dead.
* A node that failed to answer can be probed again (`--retry-dead N`, off by
  default). Only dead verdicts are retried: a measured slow node is not
  re-rolled, and a blacklisted one is a decision, not a failure.
* Speed downloads are serialized by default (`--speed-concurrency 1`). Two
  downloads at once share the local line, so a 15 Mbit/s floor measured at
  concurrency ten would really be a 15 Mbit/s floor per ten nodes.
* Cloudflare's speed endpoint is tried first; large-file mirrors are fallbacks
  for exits Cloudflare throttles.