# Changelog

## Unreleased

### Changed
- Docs reorganized: `DESIGN.md` moved to `docs/design.md`; `.claude/*.md` (vision / synthesis_and_p2p_design / p2p_detailed_design / addictive_ideas / implementation_roadmap) moved under `docs/` with lowercase filenames. The `.claude/` directory was removed.

## v0.1.3 - 2026-05-15

### Added
- Dashboard login bonus with escalating streak rewards, guaranteed ticket inventory, and local-date daily claim handling.

### Changed
- TUI visuals now follow `docs/design.md` more closely across Dashboard, Achievements, Stats, and Synthesis.
- Stats tab now includes sparkline/bar-chart based monitoring views instead of text-only summaries.

### Fixed
- Login streak / daily claim logic now uses a single local-day boundary derived from one timestamp.
- Documentation was synced with the shipped TUI layout and dashboard behavior.
