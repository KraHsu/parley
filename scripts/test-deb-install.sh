#!/usr/bin/env bash
set -euo pipefail
# All package changes run in a disposable container, never on the host.
package=$(realpath "${1:?Usage: test-deb-install.sh new.deb [legacy.deb]}")
mounts=(-v "$package:/packages/new.deb:ro")
legacy=false
if [[ -n ${2:-} ]]; then
  old_package=$(realpath "$2")
  mounts+=(-v "$old_package:/packages/old.deb:ro")
  legacy=true
fi
docker run --rm -i "${mounts[@]}" ubuntu:24.04 bash -s -- "$legacy" <<'CONTAINER'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y /packages/new.deb
apt-get check
parley-cli --help
if ldd /usr/lib/parley-desktop/parley | grep -q 'not found'; then exit 1; fi
if [[ $1 == true ]]; then
  apt-get remove -y parley-desktop
  apt-get install -y /packages/old.deb
  test "$(dpkg-query -W -f='${Version}' parley)" = 0.3.0
  # Explicit removal avoids APT satisfying Conflicts by upgrading to KDE first.
  apt-get install -y /packages/new.deb parley-
  apt-get check
  parley-cli --help
fi
apt-get install -y parley parley-data
apt-get check
test "$(dpkg-query -W -f='${Status}' parley-desktop)" = 'install ok installed'
test "$(dpkg-query -W -f='${Status}' parley)" = 'install ok installed'
test "$(dpkg-query -W -f='${Status}' parley-data)" = 'install ok installed'
parley-cli --help
printf '%s\n' 'Fresh installation, optional legacy migration, and KDE coexistence passed.'
CONTAINER
