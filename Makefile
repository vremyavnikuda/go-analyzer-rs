###############################################################################
# Makefile for Go-Analyzer-RS (Rust LSP server + VS Code extension)
###############################################################################

# Папка, в которой лежит сам Makefile  →  абсолютный путь
MAKEFILE_DIR := $(realpath $(dir $(lastword $(MAKEFILE_LIST))))

# Корень Rust-проекта (Makefile лежит рядом с Cargo.toml)
ROOT_DIR      := $(MAKEFILE_DIR)
VSCODE_DIR    := $(ROOT_DIR)/vscode
CARGO_TOML    := $(ROOT_DIR)/Cargo.toml

# Итоговые бинарники, которые собирает Cargo
SERVER_SRC_WIN   := $(ROOT_DIR)/target/release/go-analyzer-rs.exe
SERVER_SRC_LINUX := $(ROOT_DIR)/target/release/go-analyzer-rs

# Место, куда кладём сервер внутри расширения
SERVER_DEST      := $(VSCODE_DIR)/server/go-analyzer-rs  # без суффиксов!

###############################################################################
# Цель по умолчанию
###############################################################################
all:
	@echo "Use 'make windows' or 'make linux' to build for a specific OS"

###############################################################################
# ----------------------------- Windows build ---------------------------------
###############################################################################
windows: win-clean win-build win-copy npm compile package

win-clean:
	@echo "Cleaning Rust project (Windows)…"
	@cargo clean --manifest-path $(CARGO_TOML)

win-build:
	@echo "Building Rust server for Windows…"
	@cargo build --release --manifest-path $(CARGO_TOML)

# Копируем с расширением .exe, но без «-win» в имени
win-copy:
	@echo "Copying Windows binary…"
	@mkdir -p $(dir $(SERVER_DEST))
	@cp $(SERVER_SRC_WIN) $(SERVER_DEST).exe
	@echo "Copied to: $(SERVER_DEST).exe"

###############################################################################
# ------------------------------ Linux build ----------------------------------
###############################################################################
linux: unix-clean unix-build unix-copy npm compile package

unix-clean:
	@echo "Cleaning Rust project (Linux)…"
	@cargo clean --manifest-path $(CARGO_TOML)

unix-build:
	@echo "Building Rust server for Linux…"
	@cargo build --release --manifest-path $(CARGO_TOML)

# Копируем без расширения и без «-linux» в имени
unix-copy:
	@echo "Copying Linux binary…"
	@mkdir -p $(dir $(SERVER_DEST))
	@cp $(SERVER_SRC_LINUX) $(SERVER_DEST)
	@echo "Copied to: $(SERVER_DEST)"

###############################################################################
# ------------------------------ Общие шаги -----------------------------------
###############################################################################
npm:
	@echo "Installing Node.js dependencies…"
	@npm --prefix $(VSCODE_DIR) install

compile:
	@echo "Compiling TypeScript client…"
	@npm --prefix $(VSCODE_DIR) run compile

package:
	@echo "Packaging VS Code extension…"
	@cd $(VSCODE_DIR) && vsce package

###############################################################################
# Полная пересборка и очистка
###############################################################################
rebuild: clean all

clean:
	@echo "Cleaning Rust artifacts…"
	@cargo clean --manifest-path $(CARGO_TOML)
	@echo "Removing VS Code build artifacts…"
	@rm -rf $(VSCODE_DIR)/out $(VSCODE_DIR)/node_modules

###############################################################################
# PHONY-цели
###############################################################################
.PHONY: \
	all windows linux \
	win-clean win-build win-copy \
	unix-clean unix-build unix-copy \
	npm compile package \
	clean rebuild
