# Go Analyzer - Advanced Static Analysis Tool for Go

[![Visual Studio Marketplace Version](https://img.shields.io/visual-studio-marketplace/v/vremyavnikuda.go-analyzer)](https://marketplace.visualstudio.com/items?itemName=vremyavnikuda.go-analyzer)
[![Crates.io](https://img.shields.io/crates/v/go-analyzer.svg)](https://crates.io/crates/go-analyzer)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

[Русская версия (Russian version)](README_RU.md)

## 📋 Project Description

**Go Analyzer** is a high-performance static analysis tool for Go that provides real-time variable lifecycle analysis, data race detection, and visual code flow analysis. The project consists of an LSP server written in Rust and a VS Code extension written in TypeScript, providing seamless integration with the development environment.

The tool helps Go developers write safer and more reliable code by automatically detecting potential security issues in concurrent code, tracking variable usage, and providing visual feedback directly in the code editor.
![Go Analyzer](img/img.png)
## 🛠️ Technology Stack

### **Server Side (LSP Server)**

- **Rust** - Main programming language for high performance
- **Tower-LSP** - Framework for Language Server Protocol implementation
- **Tree-sitter** - Go syntax parser for accurate code analysis
- **Tokio** - Asynchronous runtime for request processing
- **Serde** - LSP data serialization/deserialization

### **Client Side (VS Code Extension)**

- **TypeScript** - VS Code extension development language
- **VS Code Extension API** - Editor integration
- **Node.js** - Extension runtime environment
- **LSP Client** - Communication with LSP server

### **Build and Deployment**

- **Cargo** - Rust build system and dependency management
- **npm/vsce** - VS Code extension packaging and publishing
- **Cross-platform** - Support for Windows, macOS, Linux

## 🎯 Project Goals

### **Primary Goals:**

1. **Enhance Go Code Safety**

   - Automatic detection of data races
   - Concurrent code safety analysis
   - Warnings about potential synchronization issues

2. **Improve Development Quality**

   - Variable lifecycle tracking
   - Code variable usage visualization
   - Context-dependent analysis of structs, interfaces, and methods

3. **Maximum Performance**

   - Use Rust for performance-critical operations
   - Intelligent AST tree caching
   - Adaptive analysis delay based on file size

4. **Ease of Use**
   - Seamless VS Code integration
   - Customizable color schemes and configurations
   - Intuitive user interface

### **Additional Goals:**

- **Extensibility**: Modular architecture for adding new analysis types
- **Cross-platform**: Support for all major operating systems
- **Performance**: Analyze files up to 2000 lines in under 50ms
- **Memory**: Consume less than 50MB with caching enabled

## 📦 Installation

For Go Analyzer to work correctly, you need to install two components:

### 1. 📥 VS Code Extension

Install the extension from the VS Code Marketplace:

**🔗 [Go Analyzer Extension](https://marketplace.visualstudio.com/items?itemName=vremyavnikuda.go-analyzer)**

**What is this?** For detailed information about the extension's capabilities, read the file [`vscode\README.md`](vscode/README.md)

### 2. ⚙️ LSP Server

Install the LSP server via Cargo:

```bash
cargo install go-analyzer
```

**🔗 [Go Analyzer LSP Server](https://crates.io/crates/go-analyzer)**

**What is this?** For detailed information about the LSP server, read the file [`README_CRATES.md`](README_CRATES.md)

### 3. 🔧 Troubleshooting

**If the extension cannot find the server**, follow the PATH setup instructions:

**📖 [PATH Setup Guide](doc/PATH_SETUP.md)**

This guide contains detailed instructions for:

- Windows (PowerShell, Command Prompt, GUI)
- Linux (Bash, Zsh, Fish)
- macOS (Bash/Zsh)

## ✨ Key Features

### 🔍 **Variable Lifecycle Analysis**

- Real-time tracking of variable scope and usage
- Visual highlighting of declarations, uses, and pointer operations
- Context-aware analysis for structs, interfaces, and methods

### 🚨 **Data Race Detection**

- Intelligent goroutine analysis with severity classification
- Concurrency safety warnings for shared variable access
- Synchronization detection (mutexes, channels, atomic operations)

### 🎨 **Visual Code Enhancement**

- Color-coded decorations for different variable states
- Hover information with detailed lifecycle data
- Code graph visualization showing relationships between components

### ⚡ **Performance Optimized**

- Rust-powered LSP server for maximum speed
- Adaptive debouncing based on file size
- Memory-efficient caching with automatic cleanup

## 🚀 Quick Start

1. **Install** the extension from the VS Code Marketplace
2. **Install LSP server**: `cargo install go-analyzer`
3. **Open** any Go file in your workspace
4. **Position cursor** on a variable to see lifecycle analysis
5. **Use keyboard shortcuts** for manual control:
   - `Shift+Alt+S` - Activate analyzer
   - `Shift+Alt+C` - Deactivate analyzer

## ⚙️ Configuration

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

## 📈 Performance

- **Startup Time**: < 100ms typical startup
- **Analysis Speed**: < 50ms for files up to 2000 lines
- **Memory Usage**: < 50MB with caching enabled
- **CPU Usage**: < 5% during active analysis, 0% when idle

## 🌐 Platform Support

- **Windows** 10/11 (all editions)
- **macOS** 10.14+ (Intel and Apple Silicon)
- **Linux** (Ubuntu, Debian, CentOS, Arch and other distributions)

## 🤝 Contributing

We welcome contributions to the project! Visit our [GitHub repository](https://github.com/vremyavnikuda/go-analyzer-rs) for:

- 🐛 Bug reports
- 💡 Feature suggestions
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

# _For standalone LSP server installation: `cargo install go-analyzer`_

This project is licensed under the MIT License - see the LICENSE file for details.
