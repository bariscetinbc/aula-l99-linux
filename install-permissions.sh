#!/bin/sh
# Run with: sh app/install-permissions.sh (or sh install-permissions.sh in app/).
set -eu
rule_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
sudo install -m 0644 "$rule_dir/70-aula-l99.rules" /etc/udev/rules.d/70-aula-l99.rules
sudo rm -f /etc/udev/rules.d/90-aula-l99.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --action=change --subsystem-match=hidraw
sudo udevadm trigger --action=change --subsystem-match=tty
sudo udevadm settle
printf '%s\n' 'Permission rules refreshed. Try Live data and LCD uploads again. If access is still denied, unplug and reconnect the keyboard in your local desktop session.'
