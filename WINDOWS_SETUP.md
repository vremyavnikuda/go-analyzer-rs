# Windows Setup Guide for Go Analyzer RS

## 🚨 Important for Windows Users

### **Problem**: Gopls and Go Analyzer Conflict
On Windows, the official VS Code Go extension automatically starts `gopls` (Go Language Server), which can conflict with Go Analyzer's custom LSP server, causing high CPU usage even when Go Analyzer is deactivated.

### **Solution 1: Disable Official Go Extension (Recommended)**

1. **Disable the official Go extension** in VS Code:
   - Go to Extensions (`Ctrl+Shift+X`)
   - Search for "Go" (by Google)
   - Click the gear icon → "Disable"

2. **Or configure Go extension to not interfere**:
   - Open VS Code Settings (`Ctrl+,`)
   - Search for "go.useLanguageServer"
   - Set to `false`

### **Solution 2: VS Code Workspace Settings**

Create a `.vscode/settings.json` file in your project root:

```json
{
  "go.useLanguageServer": false,
  "go.lintOnSave": "off",
  "go.buildOnSave": "off",
  "go.vetOnSave": "off",
  "go.formatTool": "goimports",
  "go.docsTool": "godoc",
  "files.associations": {
    "*.go": "go"
  },
  "goAnalyzer.enableAutoAnalysis": true,
  "goAnalyzer.autoAnalysisDelay": 300
}
```

### **Solution 3: Process Monitoring**

If you continue experiencing high CPU usage:

1. **Check running processes**:
   ```cmd
   tasklist | findstr gopls
   tasklist | findstr go-analyzer-rs
   ```

2. **Kill hanging processes**:
   ```cmd
   taskkill /f /im gopls.exe
   taskkill /f /im go-analyzer-rs.exe
   ```

3. **Restart VS Code** completely

### **Verification**

1. **Open Task Manager** (`Ctrl+Shift+Esc`)
2. **Look for processes**:
   - `gopls.exe` (should NOT be running when Go extension is disabled)
   - `go-analyzer-rs.exe` (should only run when Go Analyzer is active ✅)

### **Keyboard Shortcuts**

- **Shift+Alt+S**: Activate Go Analyzer (starts LSP server)
- **Shift+Alt+C**: Deactivate Go Analyzer (stops LSP server)

### **Status Bar Indicator**

Watch the status bar in VS Code:
- ✅ **Go Analyzer ✅**: Extension active, server running
- ❌ **Go Analyzer ❌**: Extension inactive, server stopped

### **If Problems Persist**

1. **Check VS Code settings**:
   ```
   Ctrl+Shift+P → "Preferences: Open Settings (JSON)"
   ```

2. **Look for conflicting settings**:
   ```json
   "go.useLanguageServer": true  // Should be false
   "go.alternateTools": {
     "go-langserver": "gopls"    // Remove this
   }
   ```

3. **Restart VS Code completely**:
   - Close all VS Code windows
   - Wait 5 seconds
   - Reopen VS Code

### **Performance Tips**

1. **Use Go Analyzer only when needed**:
   - Deactivate (`Shift+Alt+C`) when not analyzing
   - Activate (`Shift+Alt+S`) only for active development

2. **Adjust analysis delay**:
   ```json
   "goAnalyzer.autoAnalysisDelay": 500  // Increase for better performance
   ```

3. **Disable auto-analysis if needed**:
   ```json
   "goAnalyzer.enableAutoAnalysis": false  // Manual analysis only
   ```

## 🔧 Troubleshooting Commands

```cmd
# Check if ports are in use
netstat -an | findstr :8080

# Check Go environment
go version
go env GOPATH
go env GOROOT

# Clear VS Code extension cache
%USERPROFILE%\.vscode\extensions
```

## 📞 Support

If you continue experiencing issues:

1. **Check the console** (`Ctrl+Shift+J` in VS Code)
2. **Look for error messages** starting with "Go Analyzer"
3. **File an issue** with console output and system information

---

**Note**: These changes ensure that Go Analyzer's LSP server properly starts and stops, preventing conflicts with gopls on Windows systems.