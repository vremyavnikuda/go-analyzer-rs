# Adding Cargo to PATH - Installation Guide

This guide explains how to add the Cargo bin directory to your system PATH so that [go-analyzer](https://crates.io/crates/go-analyzer) installed via `cargo install go-analyzer` can be accessed from anywhere in your system.

## **Windows Commands**

### **PowerShell (Recommended)**

```powershell
# Add to current session
$env:PATH += ";$env:USERPROFILE\.cargo\bin"

# Add permanently to user PATH
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$env:USERPROFILE\.cargo\bin*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$env:USERPROFILE\.cargo\bin", "User")
    Write-Host "Added $env:USERPROFILE\.cargo\bin to PATH"
} else {
    Write-Host "Cargo bin directory already in PATH"
}

# Refresh current session
$env:PATH = [Environment]::GetEnvironmentVariable("PATH", "User") + ";" + [Environment]::GetEnvironmentVariable("PATH", "Machine")
```

### **Command Prompt**

```cmd
REM Add permanently to user PATH (requires restart)
setx PATH "%PATH%;%USERPROFILE%\.cargo\bin"

REM Or add to current session only
set PATH=%PATH%;%USERPROFILE%\.cargo\bin
```

### **Manual (GUI)**

```
1. Press Win + R, type "sysdm.cpl", press Enter
2. Click "Environment Variables"
3. In "User variables", select "Path" and click "Edit"
4. Click "New" and add: %USERPROFILE%\.cargo\bin
5. Click "OK" on all dialogs
6. Restart terminal/VS Code
```
___
## **Linux Commands**

### **Bash (Permanent)**

```bash
# Add to ~/.bashrc for permanent effect
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc

# Reload current session
source ~/.bashrc

# Verify
echo $PATH | grep -o '$HOME/.cargo/bin'
```

### **Zsh (if using Zsh)**

```bash
# Add to ~/.zshrc for permanent effect
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc

# Reload current session
source ~/.zshrc

# Verify
echo $PATH | grep -o '$HOME/.cargo/bin'
```

### **Fish Shell**

```fish
# Add to Fish config
fish_add_path $HOME/.cargo/bin

# Or manually add to config.fish
echo 'set -gx PATH $HOME/.cargo/bin $PATH' >> ~/.config/fish/config.fish
```

### **Universal (works for most shells)**

```bash
# Add to ~/.profile (works for all POSIX shells)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.profile

# Reload current session
source ~/.profile
```
___
## **macOS Commands**

### **Bash/Zsh (Modern macOS uses Zsh)**

```bash
# For Zsh (default on macOS Catalina+)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

# For Bash (older macOS or if switched to Bash)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bash_profile
source ~/.bash_profile
```

---

## **Verification Commands**

### **Check if Cargo bin is in PATH:**

```bash
# Linux/macOS
echo $PATH | grep -o '$HOME/.cargo/bin'

# Windows PowerShell
$env:PATH -split ';' | Where-Object { $_ -like "*\.cargo\bin*" }

# Windows Command Prompt
echo %PATH% | findstr ".cargo\bin"
```

### **Test if go-analyzer is accessible:**

```bash
# Linux/macOS
which go-analyzer       # Linux/macOS
```
___
## **Complete Installation Scripts**

### **Windows PowerShell Script**

Save as `install-go-analyzer.ps1`:

```powershell
# install-go-analyzer.ps1
Write-Host "Installing Go Analyzer..." -ForegroundColor Green

# Install via Cargo
cargo install go-analyzer

# Add to PATH if not already there
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$cargoPath = "$env:USERPROFILE\.cargo\bin"

if ($userPath -notlike "*$cargoPath*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$cargoPath", "User")
    Write-Host "Added Cargo bin to PATH" -ForegroundColor Yellow
} else {
    Write-Host "Cargo bin already in PATH" -ForegroundColor Green
}

# Refresh current session
$env:PATH = [Environment]::GetEnvironmentVariable("PATH", "User") + ";" + [Environment]::GetEnvironmentVariable("PATH", "Machine")

# Test installation
Write-Host "Testing installation..." -ForegroundColor Blue
try {
    $version = & go-analyzer --version 2>$null
    Write-Host "go-analyzer installed successfully!" -ForegroundColor Green
    Write-Host "Version: $version" -ForegroundColor Gray
} catch {
    Write-Host "Installation verification failed. Please restart your terminal." -ForegroundColor Red
}
```

### **Linux/macOS Bash Script**

Save as `install-go-analyzer.sh`:

```bash
#!/bin/bash
# install-go-analyzer.sh

echo "Installing Go Analyzer..."

# Install via Cargo
cargo install go-analyzer

# Add to PATH
CARGO_BIN="$HOME/.cargo/bin"
SHELL_RC=""

# Detect shell and set appropriate RC file
if [[ $SHELL == *"zsh"* ]]; then
    SHELL_RC="$HOME/.zshrc"
elif [[ $SHELL == *"bash"* ]]; then
    SHELL_RC="$HOME/.bashrc"
elif [[ $SHELL == *"fish"* ]]; then
    echo "Fish shell detected. Adding to PATH..."
    fish_add_path "$CARGO_BIN" 2>/dev/null || echo 'set -gx PATH $HOME/.cargo/bin $PATH' >> ~/.config/fish/config.fish
    echo "Added to Fish PATH"
    exit 0
else
    SHELL_RC="$HOME/.profile"
fi

# Check if already in PATH
if ! grep -q "\.cargo/bin" "$SHELL_RC" 2>/dev/null; then
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> "$SHELL_RC"
    echo "Added Cargo bin to PATH in $SHELL_RC"
else
    echo "Cargo bin already in PATH"
fi

# Reload current session
export PATH="$HOME/.cargo/bin:$PATH"

# Test installation
echo "Testing installation..."
if command -v go-analyzer >/dev/null 2>&1; then
    echo "go-analyzer installed successfully!"
    echo "Version: $(go-analyzer --version 2>/dev/null || echo 'Version check failed')"
else
    echo "Installation verification failed. Please restart your terminal or run:"
    echo "source $SHELL_RC"
fi
```
___
## **Usage Instructions for Extension Users**

### **Quick Setup**

1. **Install the VS Code extension** from the marketplace
2. **Install the LSP server**: `cargo install go-analyzer`
3. **Add Cargo bin to PATH** (choose your platform):

**Windows (PowerShell):**

```powershell
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
```

**Linux/macOS:**

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

4. **Restart VS Code**
5. **Open a Go file** and start analyzing!

## **Troubleshooting**

### **Binary not found**

- Ensure `cargo install go-analyzer` completed successfully
- Verify Cargo bin is in PATH using verification commands above
- Restart your terminal/VS Code after PATH changes

### **Permission Issues**

- On Linux/macOS: Ensure `~/.cargo/bin` directory exists and is readable
- On Windows: Run PowerShell as Administrator if needed

### **Custom Installation Path**

If you have a custom Cargo installation, set the `GO_ANALYZER_PATH` environment variable:

```bash
# Point to your custom binary location
export GO_ANALYZER_PATH="/path/to/your/go-analyzer"
```

---

**These commands will ensure that `go-analyzer` is accessible from anywhere in the system, making the VS Code extension work seamlessly with the `cargo install`ed binary.**
