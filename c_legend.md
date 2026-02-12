# Go Analyzer: Color Legend

Colors represent variable event types. You can override them in VS Code settings (`goAnalyzer.*Color`).

- `Declaration` - variable declaration (default: green, `goAnalyzer.declarationColor`).
- `Use` - regular variable usage (default: yellow, `goAnalyzer.useColor`).
- `Pointer` - pointer/pointer-like operation (default: blue, `goAnalyzer.pointerColor`).
- `AliasReassigned` - reassignment/value overwrite (default: purple, `goAnalyzer.aliasReassignedColor`).
- `AliasCaptured` - variable captured by closure/goroutine (default: magenta, `goAnalyzer.aliasCapturedColor`).
- `Race` - potential data race (default: red, `goAnalyzer.raceColor`).
- `RaceLow` - low-priority race (synchronization detected) (default: orange, `goAnalyzer.raceLowColor`).

## Diagnostics UI (Struct Fields)

For struct field analysis, the primary UI is `Diagnostic` (underline + message) and `Hover`.
New cases do not introduce extra token colors; they reuse current `Race/RaceLow`.

- `FieldRaceHigh` - `Diagnostic: Warning` + `Race` (red).
- `FieldRaceLow` - no `Diagnostic` by default, only `RaceLow` (orange) + `Hover`.
- `MixedAtomic` - `Diagnostic: Warning` + explanation in `Hover`.
- `HeavyUnderLock` - `Diagnostic: Information` (or `Hover` only).
- `ReadBeforeWrite` / `WriteOnly` - `Diagnostic: Information/Warning` depending on confidence.
- `Retention` - `Diagnostic: Information` + explanation in `Hover`.
- `LargeStructCopy` - `Diagnostic: Information` + explanation in `Hover`.

Conflict priority on the same token:

- `FieldRaceHigh` > `FieldRaceLow` > `AliasReassigned` > `AliasCaptured` > `Pointer` > `Use` > `Declaration`.

Noise-control rule:

- diagnostics are shown only for `confidence=high`;
- `low/uncertain` cases stay in `Hover` and color highlighting.
