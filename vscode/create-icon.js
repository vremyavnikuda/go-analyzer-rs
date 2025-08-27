const fs = require('fs');
const path = require('path');

console.log('📋 SVG Icon Created for Go Analyzer Extension');
console.log('===========================================');
console.log('');
console.log('✅ SVG icon created: vscode/icon.svg');
console.log('');
console.log('🔄 To convert to PNG (required for VS Code), you can:');
console.log('');
console.log('1. Online converter:');
console.log('   - Go to https://cloudconvert.com/svg-to-png');
console.log('   - Upload icon.svg');
console.log('   - Set size to 128x128');
console.log('   - Download as icon.png');
console.log('');
console.log('2. Using Inkscape (if installed):');
console.log('   inkscape --export-png=icon.png --export-width=128 --export-height=128 icon.svg');
console.log('');
console.log('3. Using ImageMagick (if installed):');
console.log('   magick convert -background transparent -size 128x128 icon.svg icon.png');
console.log('');
console.log('📁 Save the resulting PNG as: vscode/icon.png');
console.log('📝 Then update package.json to include: "icon": "icon.png"');

// Check if we're in the right directory
const vscodeDir = path.join(__dirname);
const svgPath = path.join(vscodeDir, 'icon.svg');

if (fs.existsSync(svgPath)) {
    console.log('');
    console.log('✅ SVG file confirmed at:', svgPath);
} else {
    console.log('');
    console.log('❌ SVG file not found. Please run this from the vscode directory.');
}