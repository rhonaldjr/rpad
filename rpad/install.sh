#!/bin/bash
set -e

echo "Building release binary..."
cargo build --release

echo "Installing rpad..."

# Install binary
sudo cp target/release/rpad /usr/local/bin/

# Install icon
sudo mkdir -p /usr/local/share/icons/hicolor/scalable/apps/
sudo cp assets/rpad_icon.svg /usr/local/share/icons/hicolor/scalable/apps/rpad_icon.svg

# Install desktop file
sudo cp rpad.desktop /usr/local/share/applications/

# Update icon cache
sudo gtk-update-icon-cache /usr/local/share/icons/hicolor/ || true

echo "Installation complete! You can now launch 'Rust Pad' from your application menu."
