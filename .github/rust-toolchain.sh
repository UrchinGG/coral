#!/usr/bin/env bash
# Keeps the Rust toolchain shared by every runner on this host on the latest stable.
#
# All runners share one ~/.cargo and ~/.rustup, and rustup rewrites the binaries there, so
# running cargo or rustc while an update is in progress fails with "Text file busy". Every
# cargo command in CI therefore holds a shared lock on $RUST_LOCK, and this script takes it
# exclusively for the update itself: the update waits for running builds to finish, and new
# builds wait for the update. If builds keep the lock busy for too long, the update is
# skipped for this run and retried on the next one.
set -eu

lock=${RUST_LOCK:?RUST_LOCK must name the toolchain lock file}
wait_secs=${RUST_LOCK_WAIT_SECS:-600}

outdated() {
    rustup check 2>/dev/null | grep -i '^stable-.*update available' >/dev/null && return 0
    for component in rustfmt clippy; do
        rustup component list --installed | grep "^$component" >/dev/null || return 0
    done
    return 1
}

exec 9>"$lock"

flock -s 9
if ! outdated; then
    echo "Rust stable is up to date."
    exit 0
fi
flock -u 9

echo "Rust stable is outdated; waiting up to ${wait_secs}s for running builds to finish."
if ! flock -x -w "$wait_secs" 9; then
    echo "::warning::Builds held the Rust toolchain lock for ${wait_secs}s; skipping the update this run."
    exit 0
fi

# Another run may have updated it while this one waited.
if outdated; then
    rustup update stable --no-self-update
    rustup component add rustfmt clippy
fi
