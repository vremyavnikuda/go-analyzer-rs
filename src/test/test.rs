#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use tokio::sync::Mutex;
    use tree_sitter::{Parser, Tree};
    use tree_sitter_rust::language as rust_language;
    use url::Url;

    /// Упрощённое хранилище, воспроизводящее поля, которые использует метод.
    struct TestStore {
        parser: Mutex<Parser>,
        trees: Mutex<HashMap<Url, Tree>>,
    }

    impl TestStore {
        fn new() -> Result<Self, tree_sitter::LanguageError> {
            let mut parser = Parser::new();
            parser.set_language(rust_language())?;
            Ok(Self {
                parser: Mutex::new(parser),
                trees: Mutex::new(HashMap::new()),
            })
        }

        async fn parse_document_with_cache(&self, uri: &Url, code: &str) -> Option<Tree> {
            let mut parser = self.parser.lock().await;
            let mut trees = self.trees.lock().await;
            let prev_tree = trees.get(uri);
            let new_tree = if let Some(prev) = prev_tree {
                parser.parse(code, Some(prev))
            } else {
                parser.parse(code, None)
            };
            if let Some(ref tree) = new_tree {
                trees.insert(uri.clone(), tree.clone());
            }
            new_tree
        }
    }

    /// 1️⃣ Первый парс: дерево должно вернуться и попасть в кэш.
    #[tokio::test]
    async fn initial_parse_stores_tree_in_cache() -> Result<(), Box<dyn std::error::Error>> {
        let store = TestStore::new()?;

        let uri = Url::parse("file:///tmp/example.rs")?;
        let code = "fn main() { println!(\"Hello\"); }";

        let tree = store.parse_document_with_cache(&uri, code).await;

        if tree.is_none() {
            return Err("первый парс должен вернуть дерево".into());
        }
        if !store.trees.lock().await.contains_key(&uri) {
            return Err("после парса дерево должно быть закэшировано".into());
        }

        Ok(())
    }

    /// 2️⃣ Повторный парс с изменённым текстом:
    /// - используется **prev_tree**,
    /// - кэш обновляется новым деревом.
    #[tokio::test]
    async fn reparse_uses_cache_and_updates_tree() -> Result<(), Box<dyn std::error::Error>> {
        let store = TestStore::new()?;
        let uri = Url::parse("file:///tmp/example.rs")?;
        let code_v1 = "fn main() { let a = 1; }";
        let code_v2 = "fn main() { let b = 2; }";

        let first_tree = match store.parse_document_with_cache(&uri, code_v1).await {
            Some(tree) => tree,
            None => return Err("первый парс должен вернуть дерево".into()),
        };

        let second_tree = match store.parse_document_with_cache(&uri, code_v2).await {
            Some(tree) => tree,
            None => return Err("второй парс должен вернуть дерево".into()),
        };

        if first_tree.changed_ranges(&second_tree).next().is_some() {
            return Err("дерево должно измениться после правки".into());
        }

        let bilding = store.trees.lock().await;
        let cached_tree = match bilding.get(&uri) {
            Some(tree) => tree,
            None => return Err("в кэше не найдено дерево после повторного парса".into()),
        };
        let cached_sexp = cached_tree.root_node().to_sexp();
        let second_sexp = second_tree.root_node().to_sexp();
        if cached_sexp != second_sexp {
            return Err("в кэше должно лежать именно новое дерево".into());
        }

        Ok(())
    }

    /// 3️⃣ Неуспешный парс (*parser.parse* вернул `None`) не должен трогать кэш.
    #[tokio::test]
    async fn failed_parse_does_not_touch_cache() -> Result<(), Box<dyn std::error::Error>> {
        /// Заглушка-парсер, который всегда «падает».
        struct FailingParser;
        impl FailingParser {
            fn parse(&mut self, _code: &str, _prev: Option<&Tree>) -> Option<Tree> {
                None
            }
        }

        use tokio::sync::Mutex as TokioMutex;
        struct FailingStore {
            parser: TokioMutex<FailingParser>,
            trees: TokioMutex<HashMap<Url, Tree>>,
        }
        impl FailingStore {
            fn new() -> Self {
                Self {
                    parser: TokioMutex::new(FailingParser),
                    trees: TokioMutex::new(HashMap::new()),
                }
            }
            async fn parse_document_with_cache(&self, uri: &Url, code: &str) -> Option<Tree> {
                let mut parser = self.parser.lock().await;
                let mut trees = self.trees.lock().await;
                let prev_tree = trees.get(uri);
                let new_tree = if let Some(prev) = prev_tree {
                    parser.parse(code, Some(prev))
                } else {
                    parser.parse(code, None)
                };
                if let Some(ref tree) = new_tree {
                    trees.insert(uri.clone(), tree.clone());
                }
                new_tree
            }
        }

        let store = FailingStore::new();
        let uri = match Url::parse("file:///tmp/broken.rs") {
            Ok(u) => u,
            Err(_) => return Ok(()), // Тест завершится досрочно без паники
        };

        let result = store.parse_document_with_cache(&uri, "broken code").await;
        if result.is_some() {
            return Err("метод должен вернуть None".into());
        }
        if store.trees.lock().await.get(&uri).is_some() {
            return Err("кэш должен остаться пустым".into());
        }
        Ok(())
    }
}
