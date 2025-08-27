# Go Analyzer - Advanced Static Analysis for Go

[![Visual Studio Marketplace Version](https://img.shields.io/visual-studio-marketplace/v/vremyavnikuda.go-analyzer)](https://marketplace.visualstudio.com/items?itemName=vremyavnikuda.go-analyzer)
[![Visual Studio Marketplace Downloads](https://img.shields.io/visual-studio-marketplace/d/vremyavnikuda.go-analyzer)](https://marketplace.visualstudio.com/items?itemName=vremyavnikuda.go-analyzer)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Go Analyzer** is an advanced static analysis tool that provides real-time variable lifecycle tracking, data race detection, and visual code flow analysis for Go development in VS Code.
![Go Analyzer](img/img.png)

## ✨ Features

### 🔍 **Variable Lifecycle Analysis**

- **Real-time tracking** of variable scope and usage
- **Visual highlighting** of declarations, uses, and pointer operations
- **Context-aware analysis** for structs, interfaces, and methods

### 🚨 **Data Race Detection**

- **Intelligent goroutine analysis** with severity classification
- **Concurrency safety warnings** for shared variable access
- **Synchronization detection** (mutexes, channels, atomic operations)

### 🎨 **Visual Code Enhancement**

- **Color-coded decorations** for different variable states
- **Hover information** with detailed lifecycle data
- **Code graph visualization** showing relationships between components

### ⚡ **Performance Optimized**

- **Rust-powered LSP server** for maximum speed
- **Adaptive debouncing** based on file size
- **Memory-efficient caching** with automatic cleanup

## 🚀 Quick Start

1. **Install** the extension from the VS Code marketplace
2. **Install LSP server**: `cargo install go-analyzer` (optional - extension includes bundled binary)
3. **Open** any Go file in your workspace
4. **Position cursor** on a variable to see lifecycle analysis
5. **Use keyboard shortcuts** for manual control:
   - `Shift+Alt+S` - Activate analyzer
   - `Shift+Alt+C` - Deactivate analyzer

## 📋 Commands

| Command                             | Description                         | Shortcut      |
| ----------------------------------- | ----------------------------------- | ------------- |
| `Go Analyzer: Show Lifecycle`       | Analyze variable at cursor position | -             |
| `Go Analyzer: Show Graph`           | Display code relationship graph     | -             |
| `Go Analyzer: Activate Extension`   | Enable real-time analysis           | `Shift+Alt+S` |
| `Go Analyzer: Deactivate Extension` | Disable analysis (save resources)   | `Shift+Alt+C` |

_Note: Keyboard shortcuts only work when a Go file is focused._

## 🎛️ Configuration

Customize the analyzer behavior through VS Code settings:

```json
{
  "goAnalyzer.enableAutoAnalysis": true,
  "goAnalyzer.autoAnalysisDelay": 300,
  "goAnalyzer.declarationColor": "green",
  "goAnalyzer.useColor": "yellow",
  "goAnalyzer.pointerColor": "blue",
  "goAnalyzer.raceColor": "red",
  "goAnalyzer.raceLowColor": "orange",
  "goAnalyzer.aliasReassignedColor": "purple",
  "goAnalyzer.aliasCapturedColor": "magenta"
}
```

### Configuration Options

- **`enableAutoAnalysis`** - Enable automatic analysis on cursor movement
- **`autoAnalysisDelay`** - Debounce delay in milliseconds (default: 300ms)
- **Color settings** - Customize highlighting colors for different variable states

## 📊 Status Indicators

The extension provides clear visual feedback:

- **Status Bar**: Shows `Go Analyzer ✅` when active, `Go Analyzer ❌` when inactive
- **Tooltip**: Displays entity counts (variables, functions, channels, goroutines)
- **Decorations**: Color-coded underlines for different variable states

## 🎯 Use Cases

### **Concurrency Safety**

```go
func concurrent() {
    counter := 0
    go func() {
        counter++    // ⚠️ Potential race detected
    }()
    counter++        // 🚨 Race condition identified
}
```

### **Variable Lifecycle**

```go
func example() {
    x := 42          // 🟢 Declaration detected
    println(x)       // 🟡 Usage tracked
    x = 100          // 🟣 Reassignment identified
    ptr := &x        // 🔵 Pointer operation detected
}
```

### **Synchronization Analysis**

```go
func synchronized() {
    var mu sync.Mutex
    counter := 0
    go func() {
        mu.Lock()
        counter++    // Safe: synchronized access
        mu.Unlock()
    }()
}
```

## ⚙️ Requirements

- **VS Code** version 1.60.0 or higher
- **Go** programming language environment
- **Windows, macOS, or Linux** operating system

## 🔧 Technical Details

### Architecture

- **Rust LSP Server**: High-performance analysis engine
- **TypeScript Client**: VS Code integration and UI
- **Tree-sitter**: Accurate Go syntax parsing
- **LSP Protocol**: Standard language server communication

### Performance

- **Startup Time**: < 100ms typical
- **Analysis Speed**: < 50ms for files up to 2000 lines
- **Memory Usage**: < 50MB with caching enabled
- **CPU Usage**: < 5% during analysis, 0% when idle

## 🌐 Language Support / Поддержка языков

### English

This extension provides comprehensive Go code analysis with real-time feedback and visual enhancements to improve code safety and maintainability.

### Русский

Данное расширение обеспечивает комплексный анализ Go кода с обратной связью в реальном времени и визуальными улучшениями для повышения безопасности и поддерживаемости кода.

## 🐛 Troubleshooting

### **Extension Not Working**

1. Ensure you have Go files open
2. Check status bar for activation state
3. Try manual activation with `Shift+Alt+S`

### **Performance Issues**

1. Increase `autoAnalysisDelay` for large files
2. Temporarily deactivate with `Shift+Alt+C`
3. Close unused Go files

### **LSP Server Issues**

1. **Binary not found**: Install with `cargo install go-analyzer`
2. **Custom installation**: Set `GO_ANALYZER_PATH` environment variable
3. **Restart VS Code** after installing the server
4. **Check for conflicting Go extensions**
5. **Verify file permissions**

## 🤝 Contributing

We welcome contributions! Please visit our [GitHub repository](https://github.com/vremyavnikuda/go-analyzer-rs) for:

- 🐛 Bug reports
- 💡 Feature requests
- 🔧 Pull requests
- 📖 Documentation improvements

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](https://github.com/vremyavnikuda/go-analyzer-rs/blob/main/LICENSE) file for details.

## 🙏 Acknowledgments

- **Tree-sitter** for Go syntax parsing
- **Tower-LSP** for LSP server framework
- **VS Code team** for excellent extension API
- **Go community** for inspiration and feedback

---

**Made with ❤️ for the Go community**

_For standalone LSP server installation: `cargo install go-analyzer`_
