# Makefile для сборки и упаковки Go Analyzer

# Основные директории
ROOT_DIR := C:\repository\go-analyzer-rs
VSCODE_DIR := $(ROOT_DIR)\vscode
SERVER_SRC := $(ROOT_DIR)\target\release\go-analyzer-rs.exe
SERVER_DEST := $(VSCODE_DIR)\server\go-analyzer-rs.exe

# Цель по умолчанию: собрать и упаковать всё
all: clean build copy npm compile package

# Очистка Rust проекта
clean:
	@echo "Cleaning Rust project..."
	@cd $(ROOT_DIR) && cargo clean

# Сборка Rust сервера
build:
	@echo "Building Rust server..."
	@cd $(ROOT_DIR) && cargo build --release

# Копирование бинарника в папку vscode\server
copy:
	@echo "Copying server binary..."
	@echo "Checking folder: $(VSCODE_DIR)\server"
	@if not exist "$(VSCODE_DIR)\server" (echo Creating folder && mkdir "$(VSCODE_DIR)\server") else (echo Folder exists)
	@echo "Copying from $(SERVER_SRC) to $(SERVER_DEST)"
	@copy $(SERVER_SRC) $(SERVER_DEST)

# Установка Node.js зависимостей
npm:
	@echo "Installing Node.js dependencies..."
	@cd $(VSCODE_DIR) && npm install

# Компиляция TypeScript клиента
compile:
	@echo "Compiling TypeScript client..."
	@cd $(VSCODE_DIR) && npm run compile

# Упаковка VS Code расширения
package:
	@echo "Packaging VS Code extension..."
	@cd $(VSCODE_DIR) && vsce package

# Очистка и полная пересборка
rebuild: clean all

.PHONY: all clean build copy npm compile package rebuild