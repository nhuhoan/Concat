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
| 5 | `speech.download` fetches a whisper/TTS model headlessly through the shared engine | `concat-api/src/{lib.rs,message.rs}` | `feat(api): add speech.download to fetch models headlessly` | none | **ours — active** |
| 6 | `clip.enhance` / `clip.reverse` — AI upscale/restore and reverse a clip over the socket (share enhancers/reversers) | `concat-api/src/{lib.rs,message.rs}`, `concat/src/panes/settings.rs` | `feat(api): expose clip.enhance/clip.reverse and speech model management` | none | **ours — active** |
| 7 | `speech.deleteModel` / `speech.cancelDownload` — round out model management | `concat-api/src/{lib.rs,message.rs}` | (same commit as #6) | none | **ours — active** |
| 8 | `media.peaks` exposes a file's audio waveform (min/max per bucket) for client-side silence/beat detection | `concat-api/src/{lib.rs,message.rs}` | `feat(api): add media.peaks to expose a file's audio waveform` | none | **ours — active** |
| 9 | `caption.build` — one-call auto-captions: timed segments → a batch of styled caption clips on the active timeline (presets: `phrase`, `popWord`; transcribes first if asked). The "CapCut Auto Captions" of the MCP. | `concat-api/src/{lib.rs,message.rs}`, `concat-speech/src/transcribe.rs` (Deserialize on Segment/Word) | `feat(api): add caption.build for one-call styled auto-captions` | none | **ours — active** |
| 10 | `caption.build` `animate` flag — pops each caption in with a short scale keyframe (places clips one at a time for the minted id). | `concat-api/src/{lib.rs,message.rs}` | `feat(api): pop and karaoke captions in caption.build` | none | **ours — active** |
| 11 | `caption.build` `karaoke` preset + `concat.karaoke` effect package — a line lights up word by word as spoken; `progress` keyed from word timings, dimming unspoken words. No-op off a title (reveal-order contract), so its probes are an identity round-trip. | `concat-api/src/{lib.rs,message.rs}`, `concat-effects/packages/concat.karaoke/{effect.toml,effect.wgsl,fixtures.toml}` | (same commit as #10) | none | **ours — active** |
| — | CI manual dispatch (fork-only, never upstream) | `.github/workflows/build-app.yml` | `ci: allow manual dispatch (fork-only)` | n/a | fork-only |

> #6 and #7 share one commit (same files, interleaved). If upstream later adds
> one, split with `git rebase -i` before dropping the other.
>
> #10 and #11 likewise share one commit: both extend `caption.build`'s per-clip
> apply path in the same function, so their hunks interleave. #11 also adds the
> self-contained `concat.karaoke` package, which can be lifted out on its own.

## MCP editing-support verification

Audit of the `Command` enum (what `edit.apply` can do) vs common editing needs:

| Need | Via MCP? |
|---|---|
| Trim / split / move / remove / merge clips | ✅ TrimClip, SplitClips, MoveClips, RemoveClips, MergeClips |
| Add media / clips / text / shapes / effect layers | ✅ AddMedia, AddClip, AddTextClip, AddShapeClip, AddLayerClip |
| Fades, crop, per-clip transition | ✅ UpdateClip (fadeIn/out, crop, transition) |
| Transform (scale/pos/rotate/opacity), speed | ✅ SetClipTransform, SetClipSpeed/SpeedCurve |
| Keyframe animation (incl. volume → ducking) | ✅ SetClipKey / SetEffectKey |
| Filters / effects / transitions (57 / 21 / 20) + color grade | ✅ catalogue + AddLayerClip |
| Cutout (bg removal), freeze frame, detach/reattach audio | ✅ SetClipCutout, FreezeFrame, DetachAudio |
| Tracks / timelines / fonts management | ✅ AddTrack, AddTimeline, AddFont, … |
| Captions (word-level), TTS, enhance, reverse, model mgmt | ✅ ours (#1–#7) |
| Export, preview frame, templates, undo/redo/batch | ✅ |

**Genuinely NOT supported via MCP (candidate suggestions):**
1. ~~Auto silence-trim in-engine~~ → superseded by #8 `media.peaks` (client-side).
2. **Background generator** — a solid/gradient/noise clip (catalogue has no
   `generator` kind), so a faceless clip needs an imported image for a backdrop.
3. **Markers / chapters** — no marker command; useful for navigation and
   chapterised export.
4. **Beat / scene detection** — auto-cut to music or shot changes.

→ Everything else standard is already callable. The above are the real
additions worth making next (plus visual sourcing, which stays external).

CI verified (run on sha f6027b5): #1–#4 compile green on linux-x86_64; the
whisper-rs token-timestamp API (#3) is correct.

## Backlog (weak points to fix; see session notes)

Done:
1. ~~Word-level captions~~ → done (#3).
2. ~~`speech.models` — list downloaded models + voices~~ → done (#4).

**Re-audit (several earlier "gaps" were wrong — upstream already has them):**
- Cutout / background removal: **already** `SetClipCutout` + `AddCutoutStroke`,
  analysed automatically at export. Not a gap.
- Loudness normalize / voice enhance: **already** audio effects ("Normalize
  Loudness", "Enhance Voice"), appliable via `AddLayerClip`/clip filters.
- Music ducking: **doable** today via `SetClipKey` on `KeyProperty::Volume`.
- Transitions (20) + keyframes (scale/offset/opacity/rotation/volume): present.

Genuinely still open, ranked:
3. Export/job **event streaming** through the MCP bridge (API already emits
   events; bridge polls the output file instead). Modest value.
4. **Auto silence-trim** — detect and cut silences/breaths. Genuine gap, niche.
5. High-level **helpers** that compose the primitives: ~~caption placement from
   `Segment.words`~~ → done (#9 `caption.build`); auto-ducking a music clip
   under the voice (doable via `SetClipKey` Volume + `media.peaks`);
   slideshow-from-images. Ergonomics, not missing capability.

**Caption roadmap (expert eval, CapCut/OpenCut lens):** #9 ships the dominant
short-form looks (`phrase`, `popWord`) as one-call auto-captions; #10 adds the
pop-in; #11 adds word-by-word karaoke. Remaining follow-up:
- **Karaoke active-word _colour_** (the spoken word recolours, e.g. yellow,
  rather than just the unspoken words dimming). This breaks the "no-op
  off-title" property the identity probes rely on, so it needs real GPU probes
  authored through the display↔light colour maths — which can't be validated
  without rendering. Deferred until #11's look is confirmed on a real GPU; then
  extend `concat.karaoke` with a colour param and hand-authored probes.

> #11's shader is **display-space** on purpose: `scene_linear()` is `format >= 2`
> and only scene-linear drawn packages must pin GPU probes (`concat-effects`
> `every_scene_linear_package_probes_…`). A display-space format-2 shader still
> needs probes, but because this one is a no-op off a title (reveal-order 0),
> the probes are a plain round-trip identity — safe to author blind. If a future
> change makes it alter pixels off a title, it will need real probes.

Out of scope for the API: **visual sourcing** (stock footage / image gen) and a
solid/gradient **background generator** (catalogue has none) — Concat is an
editor, not an asset bank; bring assets or integrate a source externally.

## MCP bridge (fork-local, not in the Concat repo)

`~/Documents/concat/concat_mcp.py` wraps the socket as MCP tools. Keep its
`METHODS` list in sync when we add JSON-RPC methods (speech.transcribe/tts
already added). Token lives in `~/.claude.json` env `CONCAT_API_TOKEN`; pin a
fixed token in Concat → Settings → Automation so it survives restarts.
