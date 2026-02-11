# Go Analyzer: Легенда Цветов

Цвета соответствуют типу события для переменной. Значения можно изменить в настройках VS Code (`goAnalyzer.*Color`).

- `Declaration` — объявление переменной (по умолчанию: зелёный, `goAnalyzer.declarationColor`).
- `Use` — обычное использование переменной (по умолчанию: жёлтый, `goAnalyzer.useColor`).
- `Pointer` — указатель/операция с указателем (по умолчанию: синий, `goAnalyzer.pointerColor`).
- `AliasReassigned` — переопределение/перезапись значения (по умолчанию: фиолетовый, `goAnalyzer.aliasReassignedColor`).
- `AliasCaptured` — захват переменной в замыкание/горутину (по умолчанию: magenta, `goAnalyzer.aliasCapturedColor`).
- `Race` — потенциальная гонка данных (по умолчанию: красный, `goAnalyzer.raceColor`).
- `RaceLow` — гонка низкого приоритета (есть синхронизация) (по умолчанию: оранжевый, `goAnalyzer.raceLowColor`).
