#!/bin/bash
# Configure only the disposable GitHub-hosted runner, never a user's desktop.
set -euo pipefail

if [[ "${GITHUB_ACTIONS:-}" != true || "${RUNNER_ENVIRONMENT:-}" != github-hosted ]]; then
  echo "This setup is restricted to disposable GitHub-hosted runners." >&2
  exit 1
fi

probe() {
  bwrap --die-with-parent --unshare-net --unshare-pid \
    --ro-bind / / --dev /dev --proc /proc /bin/true
}

if ! probe; then
  # Ubuntu 24.04 permits userns creation but blocks capabilities such as
  # NET_ADMIN without an application profile. Keep the global restriction on.
  restriction=/proc/sys/kernel/apparmor_restrict_unprivileged_userns
  if [[ ! -r "$restriction" || "$(cat "$restriction")" != 1 ]]; then
    echo "Bubblewrap failed for a reason other than the expected AppArmor policy." >&2
    exit 1
  fi
  profile=$(mktemp "${RUNNER_TEMP:?}/sidera-ci-bwrap.XXXXXXXX")
  trap 'rm -f -- "$profile"' EXIT
  cat > "$profile" <<'PROFILE'
abi <abi/4.0>,
include <tunables/global>

# Replaces the bwrap profile in this runner's kernel only. Allow nested bwrap
# for startup fixtures; filesystem/network restrictions still come from bwrap.
profile bwrap /usr/bin/bwrap flags=(unconfined) {
  userns,
}
PROFILE
  sudo apparmor_parser -r "$profile"
  probe
fi

# Fail before compiling if the actual test wrapper or nested fixtures cannot run.
python3 tools/run_isolated.py --timeout 30 -- \
  bwrap --die-with-parent --ro-bind / / --unshare-net /bin/true
python3 tools/run_isolated.py --timeout 30 -- python3 -c \
  'import socket; server = socket.create_server(("127.0.0.1", 0)); client = socket.create_connection(server.getsockname(), timeout=2); peer, _ = server.accept(); peer.close(); client.close(); server.close(); print("Private loopback and nested isolation are ready")'
