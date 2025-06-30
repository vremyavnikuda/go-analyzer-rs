# Go-Analyzer-RS

all:
	@echo "Use  make build-windows  or  make build-linux"

# WINDOWS
build-windows: win-clean win-build win-copy npm compile package
	@echo "INFO: Full Windows build complete"

win-clean:
	@cargo clean
	@echo "INFO: Cleaning Rust project (Windows)"
	@del /f /q "vscode\\server\\go-analyzer-rs.exe"
	@echo "INFO: go-analyzer-rs.exe delete "vscode\server\go-analyzer-rs.exe""
	@del /f /q "vscode\\go-analyzer-0.0.1.vsix"
	@echo "INFO: go-analyzer-0.0.1.vsix delete "vscode\\go-analyzer-0.0.1.vsix""


win-build:
	@echo "INFO: Building Rust server for Windows"
	@cargo build --release

win-copy:
	@echo "INFO: Copying server binary file"
	@if not exist "vscode\\server" mkdir "vscode\\server"
	@copy /Y "target\\release\\go-analyzer-rs.exe" "vscode\\server\\go-analyzer-rs.exe"
	
	

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
	@cp target/release/go-analyzer-rs vscode/server/go-analyzer-rs

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

# «build»
ifeq ($(OS),Windows_NT)
build: build-windows
else
build: build-linux
endif

# PHONY
.PHONY: \
	all build build-windows build-linux \
	win-clean win-build win-copy \
	unix-clean unix-build unix-copy \
	npm compile package
