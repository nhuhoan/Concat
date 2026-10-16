# Fork improvements & upstream-merge strategy

Fork of **jub0t/Concat**. This file is **fork-only** — do not include it in any
pull request to upstream. It tracks every change we carry on top of upstream and
how to keep them mergeable as upstream evolves.

## Principle

Upstream may later add any of these features. On each upstream release we
**compare**:

- Upstream's version is **better** → drop ours, take theirs.
- Upstream is **worse or missing** → keep ours.

So every improvement is an **atomic, well-scoped commit** that can be rebased,
cherry-picked, or dropped independently. Changes stay **minimal and additive**
(add alongside upstream code; avoid rewriting it) to keep rebase conflicts tiny.

## Git layout

- `main` — tracks upstream exactly. Never commit here; only
  `git fetch upstream && git merge --ff-only upstream/main`.
- `feat/speech-api` — our patch series (one improvement per commit), kept
  rebased on top of `upstream/main`.
- Each improvement is also safe to split onto its own topic branch for a clean
  upstream PR.

Remotes: `origin`/`upstream` = jub0t/Concat, `fork` = nhuhoan/Concat (SSH).

## Compare-on-release workflow

```sh
git fetch upstream                     # get the new release
git checkout feat/speech-api
git rebase upstream/main               # replay our commits on top
# For each commit that now conflicts or looks redundant:
#   - upstream added an equivalent & better  -> `git rebase --skip` (drop ours)
#   - upstream differs but ours is better    -> resolve keeping ours
#   - independent                            -> resolve mechanically
# Re-run the build (CI workflow_dispatch) and the pipeline smoke test.
```
Update the **Status** column below after every compare.

## Improvements tracker

| # | Improvement | Files (main) | Commit | Upstream equiv? | Status |
|---|---|---|---|---|---|
| 1 | `speech.transcribe` + `speech.tts` JSON-RPC methods; share transcriber/voices into the socket Api; `speech` capability | `concat-api/{Cargo.toml,src/lib.rs,src/message.rs}`, `concat/src/panes/settings.rs`, `concat-speech/src/{transcribe,tts}.rs` (derives) | `feat(api): expose speech.transcribe and speech.tts` | none as of 0.2.7 | **ours — active** |
| 2 | Kokoro no longer needs Pocket TTS on disk (lazy `recordings` resolution) | `concat-speech/src/tts.rs` | `fix(speech): a Kokoro voice no longer needs Pocket TTS` | bug, none | **ours — active** |
| 3 | Word-level timestamps on captions (`Segment.words`) for karaoke captions | `concat-speech/src/transcribe.rs` | `feat(speech): word-level timestamps for captions` | none | **ours — active** |
| 4 | `speech.models` lists whisper/TTS models + voices with downloaded flag | `concat-api/src/{lib.rs,message.rs}` | `feat(api): add speech.models to list models and voices` | none | **ours — active** |
| — | CI manual dispatch (fork-only, never upstream) | `.github/workflows/build-app.yml` | `ci: allow manual dispatch (fork-only)` | n/a | fork-only |

## Backlog (weak points to fix; see session notes)

Ranked by value for a polished auto-clip:

1. ~~Word-level captions~~ → done (#3).
2. ~~`speech.models` — list downloaded models + voices~~ → done (#4).
3. Export/job **event streaming** through the MCP bridge (API already emits
   events; bridge must forward so callers stop polling the output file).
4. Audio auto-polish: silence-trim, loudness-normalize, music ducking.
5. Cutout (background removal) exposed as an `edit.apply`/API command.
6. High-level helpers (transition-between-cuts, animate-caption) — ergonomics.

Out of scope for the API: **visual sourcing** (stock footage / image gen) — a
polished clip needs good visuals, but Concat is an editor, not an asset bank;
integrate externally.

## MCP bridge (fork-local, not in the Concat repo)

`~/Documents/concat/concat_mcp.py` wraps the socket as MCP tools. Keep its
`METHODS` list in sync when we add JSON-RPC methods (speech.transcribe/tts
already added). Token lives in `~/.claude.json` env `CONCAT_API_TOKEN`; pin a
fixed token in Concat → Settings → Automation so it survives restarts.
