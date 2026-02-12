# TODO

- [ ] UI: redesign the display of additional signals (race + reassign) to make it informative and visually noise-free.
- [x] Hover text: refine race messages with read/write context to reduce ambiguity.
- [ ] goanalyzer/graph все еще торчит в API/сервере, хотя фича фактически не используется: -> удалить
- [ ] RaceLow может выставляться слишком оптимистично.достаточно любой синхронизации где-то в горутине, чтобы занизить риск. ->  подумать как сделать более надежное решение
- [ ] Потенциально рискованный block_on внутри async-обработчика: backend.rs (lines 922-924). ->  разобраться с этим 
- [ ] Битые/устаревшие метаданные:
- - [ ] badge на несуществующий workflow: README.md (line 7) (rust.yml). -> удали его тогда
- - [ ] README.md с неверным путем к картинке (img.png от корня репо). ->  это нужно исправить 
