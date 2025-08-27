const fs = require('fs');
const path = require('path');

const isWindows = process.platform === 'win32';
const serverBinaryName = isWindows ? 'go-analyzer.exe' : 'go-analyzer';

// Paths
const cargoTargetDir = path.join(__dirname, '../../target/release');
const extensionServerDir = path.join(__dirname, '../server');
const sourcePath = path.join(cargoTargetDir, serverBinaryName);
const destPath = path.join(extensionServerDir, serverBinaryName);

console.log('Copying server binary...');
console.log(`Platform: ${process.platform}`);
console.log(`Source: ${sourcePath}`);
console.log(`Destination: ${destPath}`);

// Create server directory if it doesn't exist
if (!fs.existsSync(extensionServerDir)) {
    console.log('Creating server directory...');
    fs.mkdirSync(extensionServerDir, { recursive: true });
}

// Check if source binary exists
if (!fs.existsSync(sourcePath)) {
    console.error(`Error: Server binary not found at ${sourcePath}`);
    console.error('Please run "cargo build --release" first to build the server.');
    process.exit(1);
}

try {
    // Copy the binary
    fs.copyFileSync(sourcePath, destPath);
    
    // Make it executable on Unix systems
    if (!isWindows) {
        fs.chmodSync(destPath, 0o755);
    }
    
    console.log('✅ Server binary copied successfully!');
    console.log(`Binary size: ${(fs.statSync(destPath).size / 1024 / 1024).toFixed(2)} MB`);
} catch (error) {
    console.error('❌ Error copying server binary:', error.message);
    process.exit(1);
}