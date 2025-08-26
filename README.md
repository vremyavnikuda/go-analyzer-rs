# Go Analyzer RS

[README_RU](README_RU.md)

**Production-ready Go code analyzer** - Go code analyzer in Rust with VS Code support, providing intelligent variable lifecycle tracking and advanced data race detection.

✅ **[25.08.2025](https://github.com/vremyavnikuda/go-analyzer-rs/commit/d24b2cb42eb70ef22012726f390137cd8b0b26a9)**: Stable, crash-resistant LSP server with enhanced analysis capabilities

## Features

### **Enhanced Variable Analysis**

- **Support for all Go constructs**: Variables, struct fields, interface methods, function parameters
- **Advanced scope determination**: Function level, loop variables, type switch, range clause
- **Intelligent cursor positioning**: Context-dependent detection with 15+ specialized contexts
- **Automatic real-time analysis**: Analysis with delay and configurable settings (300ms by default)

### **Stabilized**

- **Crash-resistant LSP server**: Comprehensive panic recovery and error handling
- **Memory-safe operations**: Proper asynchronous resource management and cleanup
- **Graceful degradation**: Safe fallbacks when analysis errors occur
- **Optimized performance**: Incremental parsing with AST caching

### **Stabilized: race detection**

- **Context-dependent severity levels**: Race detection with High/Medium/Low priority
- **Synchronization awareness**: Detects mutex locks and atomic operations
- **Smart goroutine analysis**: Multiple goroutine patterns (anonymous functions, method calls)
- **Variable access analysis**: Read/Write/Modify/Address-taking operation detection

### **Enhanced Visualization**

- **Extension Control**: Quick activation/deactivation with keyboard shortcuts (Shift+Alt+S/C) and visual status indicators
- **Color analysis**:
  - 🟢 **Green**: Variable declarations
  - 🟡 **Yellow**: Regular variable usage
  - 🔵 **Blue**: Pointer operations (`&var`, `*ptr`)
  - 🔴 **Red**: High-priority data races in goroutines
  - 🟠 **Orange**: Low-priority races (synchronized contexts)
  - 🟣 **Purple**: Reassigned alias variables
  - 🟪 **Magenta**: Captured variables in closures
- **Rich hover information**: Detailed variable lifecycle and race warnings
- **Intelligent highlighting**: Context-sensitive highlighting based on cursor position

## Installation

> **Note**: The LSP server is thoroughly tested and includes comprehensive error handling to ensure stability during analysis.

### Quick Start (Recommended)

Use the provided Makefile for easy building and packaging:

```bash
# Build everything and package extension (Windows)
make build-windows

# Build everything and package extension (Linux/macOS)
make build-linux

# Test the build
cargo test
```

#### Step-by-step build:

```bash
make clean    # Clean previous builds (win-clean or unix-clean)
make build    # Build Rust server
make copy     # Copy server to extension folder
make npm      # Install Node.js dependencies
make compile  # Compile TypeScript extension
make package  # Package VS Code extension
```

### Manual Installation

#### Building the server

```bash
cargo build --release
```

#### Building VS Code extension

```bash
cd vscode
npm install
npm run compile
```

#### Copying server binary

After building the Rust server, copy the binary to the extension folder:

```bash
# Create server folder if it doesn't exist
mkdir -p vscode/server

# Copy binary (Windows)
copy target\release\go-analyzer-rs.exe vscode\server\

# Copy binary (Linux/macOS)
cp target/release/go-analyzer-rs vscode/server/
```

### Development Build

For development, you can use:

```bash
# Build in debug mode
cargo build

# Build extension in watch mode
cd vscode && npm run watch
```

## Usage

### Extension Activation Control

**Keyboard Shortcuts**:

- **Shift+Alt+S**: Activate the extension (enable analysis)
- **Shift+Alt+C**: Deactivate the extension (disable analysis)

**Visual Status Indicator**: The status bar shows the current extension state:

- ✅ **Active**: Extension is running and analyzing code
- ❌ **Inactive**: Extension is disabled to save resources

> **Note**: The extension doesn't always need to be active. Use deactivation when you want to save system resources or focus on other tasks.

### Automatic Analysis (when active)

1. Open a Go file in VS Code
2. Install the extension (if not already installed)
3. Ensure the extension is active (✅ in status bar or use Shift+Alt+S)
4. Simply move the cursor over variables - analysis will start automatically
5. You'll see color indication of the entire variable lifecycle

### Manual Analysis

1. Select a variable in the code
2. Execute command `Go Analyzer: Show Lifecycle` (Ctrl+Shift+P → "Go Analyzer: Show Lifecycle")

## Configuration

In VS Code settings you can configure:

- `goAnalyzer.enableAutoAnalysis` - enable/disable automatic analysis (default: true)
- `goAnalyzer.autoAnalysisDelay` - delay before automatic analysis in milliseconds (default: 300)
- `goAnalyzer.declarationColor` - color for variable declarations (default: "green")
- `goAnalyzer.useColor` - color for variable usage (default: "yellow")
- `goAnalyzer.pointerColor` - color for pointer operations (default: "blue")
- `goAnalyzer.raceColor` - color for data race warnings (default: "red")
- `goAnalyzer.raceLowColor` - color for low-priority race warnings (default: "orange")
- `goAnalyzer.aliasReassignedColor` - color for reassigned alias variables (default: "purple")
- `goAnalyzer.aliasCapturedColor` - color for captured alias variables (default: "magenta")

## Example

```go
func main() {
    x := 42          // 🟢 Declaration
    println(x)       // 🟡 Usage
    x = 100          // 🟣 Reassignment
    ptr := &x        // 🟡 Usage of x, 🟢 Declaration of ptr
    println(*ptr)    // 🔵 Pointer usage
    go func() {
        println(x)   // 🟪 Captured variable in goroutine
    }()
}
```

## Technical Details

### Architecture

- **Server**: Rust LSP server with Go parsing via tree-sitter
- **Client**: TypeScript extension for VS Code
- **Protocol**: Language Server Protocol (LSP) via tower-lsp
- **Runtime**: Tokio async runtime for performance
- **Parsing**: Incremental AST parsing with caching

### Enhanced Features (25.08.2025)

- **Advanced Go construct support**: 10+ variable declaration patterns
- **Smart cursor detection**: 15+ context types for precise analysis
- **Robust error handling**: Panic recovery with graceful degradation
- **Safe multithreading**: Proper async mutex management
- **Context-dependent race detection**: Severity levels based on synchronization

### Performance Features

- **Delayed analysis**: Configurable delay (300ms default) prevents overload
- **Smart caching**: Reuse of parsed AST trees for efficiency
- **Incremental parsing**: Re-analysis of only changed parts
- **Resource cleanup**: Proper removal of decorations and event handlers

### Supported Go Patterns

- Variable declarations: `var x int`, `x := value`
- Function parameters and return values
- Struct field access: `obj.field`
- Interface method calls: `interface.method()`
- Range clauses: `for i, v := range slice`
- Type switches: `switch v := x.(type)`
- Goroutine analysis: `go func(){}()`, `go method()`
- Pointer operations: `&var`, `*ptr`

## Quality Assurance

### Testing

The project includes comprehensive unit tests covering all analysis functions:

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_enhanced_cursor_position_detection

# Check compilation errors
cargo check
```

**Test Coverage**:

- ✅ Variable detection accuracy
- ✅ Cursor position detection
- ✅ Goroutine analysis
- ✅ Synchronization detection
- ✅ Race severity determination
- ✅ Entity counting
- ✅ Enhanced cursor context analysis

### Stability Features

- **Panic recovery**: All analysis functions include `catch_unwind` protection
- **Resource management**: Proper async mutex handling and cleanup
- **Error logging**: Comprehensive error reporting for debugging
- **Graceful degradation**: Safe fallbacks when analysis issues occur

## Development

### Project Structure

```
go-analyzer-rs/
├── src/
│   └── main.rs          # Rust LSP server
├── vscode/
│   └── src/
│       └── extension.ts # VS Code extension
├── go_test/
│   └── main.go          # Test Go file
└── Cargo.toml
```

### Running in Development Mode

#### Using Makefile (Recommended)

```bash
# Full development build
make rebuild

# Or step by step:
make clean
make build
make copy
make npm
make compile
```

#### Manual Development Setup

1. Build server: `cargo build`
2. Copy binary: `copy target\debug\go-analyzer-rs.exe vscode\server\` (Windows) or `cp target/debug/go-analyzer-rs vscode/server/` (Linux/macOS)
3. Build extension: `cd vscode && npm run compile`
4. Run VS Code in debug mode (F5)
5. Open test Go file and test functionality

## How It Works

### Variable Lifecycle Analysis

The analyzer uses tree-sitter to parse Go code and build an Abstract Syntax Tree (AST). When a variable is selected (either in declaration or usage), the system:

1. **Finds variable declaration** in the current scope
2. **Collects all usages** throughout the function
3. **Detects pointer operations** (address-taking `&`, dereferencing `*`)
4. **Identifies usage in goroutines** for data race detection
5. **Applies color decorations** to visualize the lifecycle

### Real-time Analysis

- **Cursor tracking**: Monitors cursor position changes in Go files
- **Delayed queries**: Uses configurable delay to prevent excessive server requests
- **Smart updates**: Analysis only on actual cursor position changes
- **Resource management**: Proper cleanup of decorations and timeouts

### Data Race Detection

The analyzer detects potential data races through:

1. **Goroutine identification**: Finding `go` statements in code
2. **Variable usage in goroutines**: Checking if variables are used inside goroutines
3. **Scope analysis**: Determining if variables are shared between goroutines
4. **Visual warnings**: Marking such usage with red color

## Performance Considerations

- **Efficient parsing**: Uses tree-sitter for fast incremental parsing
- **Delayed analysis**: Configurable delay prevents excessive CPU usage
- **Smart caching**: Reuses parsed AST when possible
- **Resource cleanup**: Proper removal of decorations and event handlers

## Troubleshooting

### Common Issues

1. **Decorations not appearing**:

   - Check that file has `.go` extension
   - Ensure LSP server is running
   - Check VS Code console for errors

2. **Slow performance**:

   - Increase `autoAnalysisDelay` in settings
   - Disable automatic analysis if not needed
   - Check for large files or complex code

3. **Incorrect analysis**:
   - Ensure Go syntax is correct
   - Check for parsing errors in console
   - Verify tree-sitter grammar compatibility

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if necessary
5. Submit a pull request

## License

<<<<<<< HEAD
This project is licensed under the MIT License - see the LICENSE file for details.
=======
This project is licensed under the MIT License - see the LICENSE file for details.

> > > > > > > f2ed354cbb6e6155331a9ce417d85668e1444944
