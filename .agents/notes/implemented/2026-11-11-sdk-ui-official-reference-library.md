# SDK/UI official reference library

Status: implemented — 2026-11-11

## Problem

BerryUI and SDK decisions had relied too heavily on memory or general platform intuition. The repository had reference projects for kernel/OS architecture, but no dedicated, reproducible corpus for official SDK, UI, graphics, input, accessibility, and design-system behavior from Android, OpenHarmony/HarmonyOS, Apple, Microsoft, Google/ChromeOS/Fuchsia, Flutter, GNOME/KDE, and smaller mobile OS ecosystems.

Directly committing cloned upstream repositories would also bloat the private repository and create avoidable license and copyright exposure.

## Decision

Create a private-repo-only asset at `reference/sdk-ui/` with:

- a curated official-source manifest (`sources.toml`);
- exact official URL, source repository, license verdict, cache strategy, branch, commit pin, and sparse paths;
- a reproducible fetch/status tool (`tools/fetch.py`);
- local ignored caches under `reference/sdk-ui/cache/`;
- explicit license and distribution boundaries (`LICENSE-NOTES.md`);
- a BerryOS-oriented lookup map (`README.md`).

The cache strategy is deliberately sparse and text-first:

```text
git clone --depth 1 --filter=blob:none --no-checkout
git sparse-checkout
Gitiles +archive for huge monorepos
```

Large media/test assets and obsolete mirrors are not fetched.

## Alternatives considered

- **Commit full upstream clones into the private Git repository**: rejected; hundreds of megabytes to gigabytes of third-party blobs and unnecessary license/history burden.
- **Store only browser bookmarks**: rejected; URLs drift and there is no commit-pin or reproducible local search surface.
- **Mirror official documentation sites wholesale**: rejected; Apple/Huawei/Fluent/Material 3 pages do not grant reuse terms and some sites are login-gated or JS-rendered.
- **Use only third-party summaries**: rejected; future SDK/UI decisions must check primary official behavior.
- **Mix this corpus with kernel/OS reference projects**: rejected; SDK/UI has different source categories, cache sizes, and license risks.

## Consequences

The private repository now has 51 inventoried official sources across OpenHarmony/HarmonyOS, Android/AOSP, Material, Chromium/ChromeOS, Fuchsia, Flutter/Dart, Apple, Microsoft/WinUI, GNOME/KDE/Qt/COSMIC, webOS OSE, Tizen/EFL, Plasma Mobile, Lomiri, and Phosh.

Nineteen high-value sources are locally cached in a roughly 476 MiB ignored cache, including OpenHarmony docs/API definitions, Android framework/Compose, Material, Fuchsia, Flutter, Swift/DocC, WinUI specs/docs, GNOME HIG, and libadwaita. Link-only entries preserve official Apple/Huawei/Google/Microsoft/Tizen design portals without mirroring protected content.

Future BerryUI/SDK discussions must cite these primary sources or state that no official precedent was found. CC-BY-SA, GPL, LGPL, and proprietary material remains reference-only and must not be copied into public BerryOS documentation.

## Related

- `reference/sdk-ui/README.md`
- `reference/sdk-ui/sources.toml`
- `reference/sdk-ui/LICENSE-NOTES.md`
- `reference/README.md`
