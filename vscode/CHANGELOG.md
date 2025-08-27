# Change Log

All notable changes to the "go-analyzer" extension will be documented in this file.

## [0.1.0] - 2024-08-27

### Added

- Initial release of Go Analyzer extension
- Real-time variable lifecycle tracking
- Data race detection for concurrent Go code
- Visual code decorations with customizable colors
- LSP server integration with Rust backend
- Manual activation/deactivation controls
- Keyboard shortcuts (Shift+Alt+S/C) for extension control
- Status bar indicators with entity counts
- Hover information for detailed variable analysis
- Configurable analysis delay and color settings
- Support for structs, interfaces, methods, and goroutines
- Automatic and manual analysis modes
- Memory-efficient caching with cleanup
- Cross-platform support (Windows, macOS, Linux)

### Features

- **Variable Analysis**: Declaration, usage, reassignment detection
- **Concurrency Safety**: Goroutine and race condition analysis
- **Synchronization Detection**: Mutex, channel, atomic operations
- **Visual Enhancement**: Color-coded syntax highlighting
- **Performance Optimization**: Adaptive debouncing, efficient caching
- **User Control**: Activation/deactivation, customizable settings

### Technical

- Rust LSP server for high-performance analysis
- TypeScript VS Code extension client
- Tree-sitter Go parser for accurate syntax analysis
- Tower-LSP framework for LSP implementation
- Comprehensive test coverage (30+)
