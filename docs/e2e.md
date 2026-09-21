# real github workflow checks

run september 21, 2026, using two private consumer repositories. upstream projects were read-only. the workflows use the action through `uses: maria-rcks/release-balls@efb1c937455874a887b83c0dc6cea9d9c37a28e8`, with github's normal job tokens. the renderer and layout were unchanged.

| setup | real source | verified behavior | run |
| --- | --- | --- | --- |
| nightly artifacts | five `pingdotgg/t3code` nightlies | merger metric, mp4 + gif, output path containing spaces, transfer to another job | [passed](https://github.com/maria-rcks/release-balls-e2e-nightly/actions/runs/35548911036) |
| stable artifacts | `astral-sh/uv` 0.12.17 and 0.12.16 | author metric, gif, repeated pr links deduplicated | [passed](https://github.com/maria-rcks/release-balls-e2e-nightly/actions/runs/35548911036) |
| filtered contributors | `cli/cli` v2.101.0 | changed-line metric, mp4, two selected users | [passed](https://github.com/maria-rcks/release-balls-e2e-nightly/actions/runs/35548911036) |
| release publication | `astral-sh/uv` 0.12.1 | actual `release.published` trigger, exact old tag, two release assets, existing notes preserved | [passed](https://github.com/maria-rcks/release-balls-e2e-release/actions/runs/35548916680) |
| encoded release links | consumer repository's `e2e/v1#hotfix` release | exact copy of the final example, default private source, encoded asset links, both formats attached | [passed](https://github.com/maria-rcks/release-balls-e2e-nightly/actions/runs/35549576248) |
| private source | consumer repository's own releases | default repository/token, slash in tag, empty release; drafts, missing tags and unsupported notes fail | [passed](https://github.com/maria-rcks/release-balls-e2e-nightly/actions/runs/35548914379) |

fixture workflows: [artifact and private-source workflows](https://github.com/maria-rcks/release-balls-e2e-nightly/tree/main/.github/workflows), [publishing workflow](https://github.com/maria-rcks/release-balls-e2e-release/blob/main/.github/workflows/release.yml). the publication fixture reads uv and publishes only to the private sandbox. the reusable [example](../examples/release.yml) reads its own repository instead.

the exact-tag publishing job passed twice: 3m23s cold and 43s on rerun. the rerun kept one generated notes section, preserved the original release notes, and replaced the two assets. these are full job times, including setup, api requests, encoding and uploads.

## data and media verification

independently fetched the selected release bodies and 110 referenced prs across the three upstream repositories. checked release membership, author, merger, merge time, additions/deletions, bot exclusion, filters, and deduplicated rankings. all matched:

| source | expected leaders |
| --- | --- |
| t3code, five nightlies (27 unique prs) | juliusmarminge 14, maria-rcks 9, shivamhwp 4 merges |
| uv, two releases (24 unique prs) | zsol 7, charliermarsh 4, konstin 3 authored prs |
| github cli, selected users | williammartin 9,407; sergiou87 68 changed lines |
| uv 0.12.17 (8 unique prs) | charliermarsh 3, AlexWaygood 1, konstin 1 authored prs |
| uv 0.12.1 (17 unique prs) | charliermarsh 5, EliteTK 3, Gankra 2 authored prs |

[saved verification results](../demo/e2e-verification.json). blacksmith decoded all nine main-scenario media files and both encoded-tag assets without errors and checked dimensions, duration, and frame counts: mp4 1080×1080 / 960 frames; gif 480×480 / 240 frames; both 16 seconds. files downloaded by a downstream job and from release assets matched the original media byte for byte. remote release build, clippy, rust formatting, and workflow lint passed.

## issue found and fixed

`match: 0.12.1` selected uv `0.12.17`, because matching is intentionally a substring search. the new `tag` input fetches one exact release and overrides the count and substring filter. the release-triggered workflow now renders uv `0.12.1` correctly, even with newer matching tags available. review also caught raw tags containing `#` breaking asset links; the example now encodes tags as one url path segment. the final example passed as-is on the private `e2e/v1#hotfix` release.

## limits

these runs exercise github actions on ubuntu, not other ci providers. the private-source case has an explicitly empty release; nonempty private pr metadata was not exercised. authenticated release downloads worked; private release-asset gifs may not load through github's public image proxy. use public hosting for a reliably embedded gif. cold builds and github's per-ref cache scope are separate from subsecond render timings.
