# Go-Analyzer-RS

all:
	@echo "Use  make build-windows  or  make build-linux"

# WINDOWS
build-windows: win-clean win-build win-copy npm compile package
	@echo "INFO: Full Windows build complete"

dev-win-clean:
	@cargo clean
	@echo "INFO: Cleaning Rust project (Windows)"
	@del /f /q "vscode\\server\\go-analyzer.exe"
	@echo "INFO: go-analyzer.exe delete "vscode\server\go-analyzer.exe""
	@del /f /q "vscode\\go-analyzer-0.1.0.vsix"
	@echo "INFO: go-analyzer-0.1.0.vsix delete "vscode\\go-analyzer-0.1.0.vsix""
	@del /f /q "vscode\\go-analyzer-0.0.1.vsix"
	@echo "INFO: go-analyzer-0.0.1.vsix delete "vscode\\go-analyzer-0.0.1.vsix""


win-build:
	@echo "INFO: Building Rust server for Windows"
	@cargo build --release

win-copy:
	@echo "INFO: Copying server binary file"
	@if not exist "vscode\\server" mkdir "vscode\\server"
	@copy /Y "target\\release\\go-analyzer.exe" "vscode\\server\\go-analyzer.exe"



# LINUX
build-linux: unix-clean unix-build unix-copy npm compile package
	@echo "INFO: Full Linux build complete"

unix-clean:
	@echo "INFO: Cleaning Rust project (Linux)"
	@cargo clean

unix-build:
	@echo "INFO: Building Rust server for Linux"
	@cargo build --release

unix-copy:
	@echo "INFO: Copying Linux binary"
	@mkdir -p vscode/server
	@cp target/release/go-analyzer vscode/server/go-analyzer

# Node / VS Code
npm:
	@cd vscode && npm install
	@echo "INFO: Installing Node.js dependencies"

compile:
	@cd vscode && npm run compile
	@echo "INFO: Compiling TypeScript client"

package:
	@cd vscode && vsce package
	@echo "INFO: Packaging VS Code extension"

# VS Code Extension Only Build
build-vscode: npm compile
	@echo "INFO: Compiling TypeScript extension"
	@cd vscode && vsce package
	@echo "INFO: VS Code extension build complete (no server binary)"

# Extension Only - No Server Dependencies
extension-only:
	@echo "INFO: Building VS Code extension only (no server interaction)"
	@cd vscode && npm install
	@cd vscode && tsc -p ./
	@cd vscode && vsce package --allow-missing-repository --no-dependencies
	@echo "✅ Extension built successfully without server binary"

# VS Code Extension Build with Server Binary
build-vscode-full: npm compile
	@echo "INFO: Compiling TypeScript extension"
	@cd vscode && npm run copy-server
	@cd vscode && vsce package
	@echo "INFO: VS Code extension build complete (with server binary)"

# dev kit «build» (make dev-build)
ifeq ($(OS),Windows_NT)
dev-build: build-windows
else
dev-build: build-linux
endif

# PHONY
.PHONY: \
	all build build-windows build-linux build-vscode build-vscode-full extension-only \
	win-clean win-build win-copy \
	unix-clean unix-build unix-copy \
	npm compile package \
	publish-check publish-prep publish test fmt

# CRATES.IO PUBLICATION
publish-check: ## Verify package is ready for crates.io publication
	@echo "📋 Verifying package for crates.io..."
	@cargo fmt --check
	@cargo clippy --all-targets --all-features -- -D warnings
	@cargo test --quiet
	@cargo check
	@echo "✅ Package verification complete"

publish-prep: ## Prepare package for crates.io publication
	@echo "📦 Preparing package for publication..."
ifeq ($(OS),Windows_NT)
	@powershell -ExecutionPolicy Bypass -File publish-to-crates.ps1
else
	@chmod +x publish-to-crates.sh
	@./publish-to-crates.sh
endif

publish: publish-check ## Publish to crates.io (requires login)
	@echo "🚀 Publishing to crates.io..."
	@echo "⚠️ Make sure you have run 'cargo login <token>' first"
	@cargo publish
	@echo "✅ Published successfully!"
	@echo "📦 Users can now install with: cargo install go-analyzer"

# TESTING AND FORMATTING
test: ## Run all tests
	@echo "🧪 Running tests..."
	@cargo test --quiet
	@echo "✅ Tests passed"

fmt: ## Format code
	@echo "🎨 Formatting Rust code..."
	@cargo fmt
	@echo "✅ Code formatted"
