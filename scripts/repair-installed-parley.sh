#!/bin/bash
# Run as the desktop user; sudo is used only for APT operations.
set -euo pipefail
if [ "$(id -u)" -eq 0 ]; then
  echo '请以普通桌面用户运行此脚本，不要在脚本前加 sudo。' >&2
  exit 1
fi
if [ "$#" -ne 1 ]; then
  echo "用法：bash $0 /path/to/ParleyDesktop_VERSION_amd64.deb" >&2
  exit 1
fi
artifact=$(realpath -- "$1")
if [ "$(dpkg-deb --field "$artifact" Package)" != 'parley-desktop' ]; then
  echo '拒绝安装：需要独立包名 parley-desktop 的修复包。' >&2
  exit 1
fi
# Explicitly remove our legacy package during migration: otherwise APT can
# resolve a versioned conflict by upgrading it to KDE and hit the icon clash.
# An already-installed KDE package is kept and its interrupted transaction fixed.
legacy_version=$(dpkg-query -W -f='${Version}' parley 2>/dev/null || true)
if [ -n "$legacy_version" ] && dpkg --compare-versions "$legacy_version" lt 0.5.0; then
  sudo apt-get install -f "$artifact" parley-
else
  sudo apt-get install -f
  sudo apt-get install "$artifact"
fi
# No autoremove, force-overwrite, repository changes or personal-data deletion.
sudo apt-get check

# Preserve the existing machine-specific WebKit workaround, but point our old
# user launchers at the new executable/icon instead of KDE's /usr/bin/parley.
stamp=$(date +%Y%m%d-%H%M%S)
wrapper="$HOME/.local/bin/parley"
if [ -f "$wrapper" ] && grep -q 'Local WebKitGTK VBlank workaround' "$wrapper"; then
  cp -p -- "$wrapper" "$wrapper.before-package-rename-$stamp"
  sed -i 's|exec /usr/bin/parley "|exec /usr/bin/parley-desktop "|' "$wrapper"
fi
for name in Parley.desktop parley-tutor.desktop parley-terminal.desktop; do
  entry="${XDG_DATA_HOME:-$HOME/.local/share}/applications/$name"
  if [ -f "$entry" ] && grep -q 'WEBKIT_FORCE_VBLANK_TIMER=1 /usr/bin/parley' "$entry"; then
    cp -p -- "$entry" "$entry.before-package-rename-$stamp"
    sed -i -e 's|/usr/bin/parley\( \|$\)|/usr/bin/parley-desktop\1|' \
      -e 's|^Icon=parley$|Icon=parley-desktop|' "$entry"
  fi
done
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "${XDG_DATA_HOME:-$HOME/.local/share}/applications"
fi
printf '%s\n' '修复完成。我们的应用包名为 parley-desktop，终端伴随命令仍为 parley-cli。' \
  '已有学习数据和本机 WebKit 性能设置已保留。'
