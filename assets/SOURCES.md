# Asset provenance

Every file in `assets/` must have a line here, and CI rejects an unlisted file. The rule this exists to
enforce: no third-party artwork, no ports of the reference project's art, no ambiguous fonts.

| File | Origin | Licence |
|---|---|---|
| — (none yet) | The v0.1 icon is generated at runtime by `crates/hanglock-win/src/icon.rs` from a distance-field description in code, so there is no binary asset to licence. A designed `.ico` for the installer arrives in Phase 2 and needs its own row here. | — |

`docs/previews/*.png` are design-review images rather than product assets, and every one of them is
output of `tools/model/hanglock_ref.py` — the reference model, which shares its constants and its
painter logic with `hanglock-render`. They are listed file by file because that is what the check in
`.github/workflows/ci.yml` asks for: it looks for each filename, so a glob here would satisfy nobody.

| File | Origin | Licence |
|---|---|---|
| `docs/previews/01-settled-light.png` | `hanglock_ref.py` render at rest on a light desktop: clamp, ring, cord, plate, `10:42` `PM`. Not a screenshot of the app — it is what the shipped painter draws. | CC0 |
| `docs/previews/02-mid-swing.png` | `hanglock_ref.py` render mid-throw: the cord whips while the plate stays near level (`plate` posture). | CC0 |
| `docs/previews/03-settled-dark.png` | `hanglock_ref.py` render, same frame on a dark wallpaper: the rim is what keeps it an object instead of a hole. | CC0 |
| `docs/previews/04-seconds.png` | `hanglock_ref.py` render of `10:42:07`, the widest run the MVP face draws. | CC0 |
| `docs/previews/05-long-hang-natural.png` | `hanglock_ref.py` render at a 230 px hang in `natural` posture: livelier, still bounded. | CC0 |
| `docs/previews/06-held-locked.png` | `hanglock_ref.py` render held under the cursor in `locked` posture: level, for reduced-motion users. | CC0 |


## The face

`crates/hanglock-render/src/face_data.rs` is generated data: geometric stroke outlines for
`0 1 2 3 4 5 6 7 8 9 : . A M P S T -` and space, authored in `tools/model/hanglock_ref.py`
(`FACE_GLYPHS`) and quantised by `tools/model/gen_face.py`. It is original geometry — no font file is
bundled, referenced, traced or derived, so no font licence is needed, because there is no font family.

If a real `.ttf` is ever bundled (Phase 4's proportional text, or wider Latin coverage), it must arrive
with its upstream licence file in `assets/fonts/`, a row in the table above, and a note on whether the
generated `face_data.rs` still exists for the headless paths.

### Phase 2.5B concepts (`docs/previews/phase-2.5/`)

These are the concept previews for the visual-refinement stage, and every one of them is output of
`tools/model/hanglock_concepts.py`, which imports its canvas, anti-aliasing and palette from
`tools/model/hanglock_ref.py` and never rebinds a constant there. That separation is the point: the
reference model's solver constants are pinned by `tests/golden/trace_drag_settle.txt`, so a look
change has to be drawable without touching them, and a file that cannot reach them is the guarantee.
No concept image here is a screenshot of the app, and none of them is production code: they are
drawings of what the shipped painter *could* draw, given a bar primitive in `hanglock-render` and a
regenerated glyph set in `tools/model/gen_face.py`. Regenerate with
`python3 tools/model/hanglock_concepts.py all docs/previews/phase-2.5`.

| File | Origin | Licence |
|---|---|---|
| `docs/previews/phase-2.5/concept-slate-light.png` | Slate on a light desktop: 204x56 plate, chamfered segment face, 2 px cord, contact shadow, meridiem in the plate's own ink. | CC0 |
| `docs/previews/phase-2.5/concept-slate-dark.png` | Slate on a dark desktop: the 0.55 px rim is what keeps it an object instead of a hole. | CC0 |
| `docs/previews/phase-2.5/concept-slate-seconds.png` | Slate with `14:53:07 PM`: the cap is fitted to the plate, so nothing is painted off the edge, which is the behaviour `text_bounds` lacks today. | CC0 |
| `docs/previews/phase-2.5/concept-slate-rest.png` | Slate, 24-hour, no meridiem: the same optical centring with the suffix absent. | CC0 |
| `docs/previews/phase-2.5/concept-glass-light.png` | Glass on a light desktop: 227x52, alpha 0.86 plate, lit top band, wide faint ambient shadow instead of one dark blob. | CC0 |
| `docs/previews/phase-2.5/concept-glass-dark.png` | Glass on a dark desktop, where the top band and the rim carry the whole separation. | CC0 |
| `docs/previews/phase-2.5/concept-glass-seconds.png` | Glass with seconds: the fitted cap keeps eight glyphs and the meridiem inside the plate. | CC0 |
| `docs/previews/phase-2.5/concept-glass-rest.png` | Glass, 24-hour, no meridiem. | CC0 |
| `docs/previews/phase-2.5/concept-thread-light.png` | Thread on a light desktop: the smallest proposal, 175x48, no clamp - the cord ends in an eyelet punched into the plate's own top edge. | CC0 |
| `docs/previews/phase-2.5/concept-thread-dark.png` | Thread on a dark desktop: a 1.5 px line needs the rim more than a thicker cord does. | CC0 |
| `docs/previews/phase-2.5/concept-thread-seconds.png` | Thread with seconds, where the fit rule does most of the work. | CC0 |
| `docs/previews/phase-2.5/concept-thread-rest.png` | Thread, 24-hour, no meridiem. | CC0 |
| `docs/previews/phase-2.5/concepts-light.png` | Slate, Glass and Thread stacked on one light desktop at their proposed default scales, at rest. | CC0 |
| `docs/previews/phase-2.5/concepts-dark.png` | The same stack on a dark desktop, for the rim and shadow decisions. | CC0 |
| `docs/previews/phase-2.5/concepts-seconds.png` | The same stack with `14:53:07 PM`, to compare how each absorbs eight glyphs. | CC0 |
| `docs/previews/phase-2.5/slate-scale-ladder.png` | Slate at scale 0.75, 1.00, 1.40 and 1.75 with the same hang: corner radius, mount, eyelet, cord and cap all grow with the card, which is the property `Settings::card()` does not have. | CC0 |
| `docs/previews/phase-2.5/face-concepts.png` | The proposed face: the digit set at two sizes, then three rows showing the fit rule at 5, 8 and 5 glyphs on one card width. | CC0 |
| `docs/previews/phase-2.5/footprint.png` | Today's swept window (red, 520x269 around a 252x96 card, containing the shipped render) beside each concept's, from `placement::swept_box`'s own formula. | CC0 |
