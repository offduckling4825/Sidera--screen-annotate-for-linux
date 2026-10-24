#!/bin/bash
set -eu
cd -- "$(dirname -- "$0")"
install -d /usr/local/share/sidera-wayland-lab /usr/local/share/applications
install -m 0644 panel.py sway.conf /usr/local/share/sidera-wayland-lab/
install -m 0755 launcher.py /usr/local/bin/sidera-wayland-lab
install -m 0644 sidera-wayland-lab.desktop sidera-wayland-lab-stop.desktop /usr/local/share/applications/
update-desktop-database /usr/local/share/applications
