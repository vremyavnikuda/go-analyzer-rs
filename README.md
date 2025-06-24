# Go Analyzer RS

[README_RU](README_RU.md)

A Go code analyzer written in Rust with VS Code support that shows variable lifecycles and warns about potential data races.

## Features

- **Variable lifecycle analysis**: When selecting a variable anywhere in the code, shows its complete lifecycle
- **Real-time automatic analysis**: Analysis runs automatically when moving the cursor
- **Pointer detection**: Automatically detects pointer usage
- **Data race detection**: Warns about potential data races in goroutines
- **Color-coded visualization**:
  - 🟢 **Green**: Variable declaration
  - 🟡 **Yellow**: Regular variable usage
  - 🔵 **Blue**: Pointer usage
  - 🔴 **Red**: Potential data race in goroutine

## Installation

### Quick Start (Recommended)

Use the provided Makefile for easy building and packaging:

```bash
# Build everything and package the extension
make all

# Or step by step:
make clean    # Clean previous builds
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

#### Building the VS Code extension

```bash
cd vscode
npm install
npm run compile
```

#### Copying server binary

After building the Rust server, copy the binary to the extension folder:

```bash
# Create server directory if it doesn't exist
mkdir -p vscode/server

# Copy the binary (Windows)
copy target\release\go-analyzer-rs.exe vscode\server\

# Copy the binary (Linux/macOS)
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

### Automatic analysis (default)
1. Open a Go file in VS Code
2. Install the extension (if not already installed)
3. Simply move the cursor over variables - analysis will start automatically
4. See color-coded visualization of the complete variable lifecycle

### Manual analysis
1. Select a variable in the code
2. Execute the command `Go Analyzer: Show Lifecycle` (Ctrl+Shift+P → "Go Analyzer: Show Lifecycle")

## Configuration

In VS Code settings, you can configure:

- `goAnalyzer.enableAutoAnalysis` - enable/disable automatic analysis (default: true)
- `goAnalyzer.autoAnalysisDelay` - delay before automatic analysis in milliseconds (default: 300)
- `goAnalyzer.declarationColor` - color for variable declarations (default: "green")
- `goAnalyzer.useColor` - color for variable usage (default: "yellow")
- `goAnalyzer.pointerColor` - color for pointer operations (default: "blue")
- `goAnalyzer.raceColor` - color for data race warnings (default: "red")

## Example

```go
func main() {
    x := 42          // 🟢 Declaration
    println(x)       // 🟡 Usage
    ptr := &x        // 🟡 Usage of x, 🟢 Declaration of ptr
    println(*ptr)    // 🔵 Pointer usage
    go func() {
        println(x)   // 🔴 Data race!
    }()
}
```

## Technical Details

- **Server**: Rust using tree-sitter for Go parsing
- **Client**: TypeScript extension for VS Code
- **Protocol**: Language Server Protocol (LSP)
- **Performance**: Automatic analysis with delay to avoid frequent requests

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

1. Build the server: `cargo build`
2. Copy the binary: `copy target\debug\go-analyzer-rs.exe vscode\server\` (Windows) or `cp target/debug/go-analyzer-rs vscode/server/` (Linux/macOS)
3. Build the extension: `cd vscode && npm run compile`
4. Launch VS Code in debug mode (F5)
5. Open the test Go file and test the functionality

## How It Works

### Variable Lifecycle Analysis
The analyzer uses tree-sitter to parse Go code and build an Abstract Syntax Tree (AST). When a variable is selected (either in declaration or usage), the system:

1. **Finds the variable declaration** in the current scope
2. **Collects all usages** throughout the function
3. **Detects pointer operations** (address-of `&`, dereference `*`)
4. **Identifies goroutine usage** for data race detection
5. **Applies color-coded decorations** to visualize the lifecycle

### Real-time Analysis
- **Cursor tracking**: Monitors cursor position changes in Go files
- **Debounced requests**: Uses configurable delay to prevent excessive server requests
- **Smart updates**: Only analyzes when cursor position actually changes
- **Resource management**: Properly cleans up decorations and timeouts

### Data Race Detection
The analyzer detects potential data races by:
1. **Identifying goroutines**: Finding `go` statements in the code
2. **Variable usage in goroutines**: Checking if variables are used inside goroutines
3. **Scope analysis**: Determining if variables are shared between goroutines
4. **Visual warnings**: Marking such usages with red color

## Performance Considerations

- **Efficient parsing**: Uses tree-sitter for fast, incremental parsing
- **Debounced analysis**: Configurable delay prevents excessive CPU usage
- **Smart caching**: Reuses parsed AST when possible
- **Resource cleanup**: Proper disposal of decorations and event listeners

## Troubleshooting

### Common Issues

1. **No decorations appear**:
   - Check if the file has `.go` extension
   - Ensure the LSP server is running
   - Check VS Code console for errors

2. **Slow performance**:
   - Increase `autoAnalysisDelay` in settings
   - Disable automatic analysis if not needed
   - Check for large files or complex code

3. **Incorrect analysis**:
   - Ensure Go syntax is correct
   - Check for parsing errors in the console
   - Verify tree-sitter grammar compatibility

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details. 