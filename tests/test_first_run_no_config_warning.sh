#!/usr/bin/env bash

# On a virgin system there is no config file yet. A missing config must be
# treated as "first run, use defaults" — silently — while a corrupt one must
# still produce a warning (it changes behavior without the user knowing).

dfm ignore .config/dfm >/dev/null 2>&1 || true

# init on the first run: no config exists yet, so no warning may appear
OUT=$(dfm init dotfiles 2>&1)
assert_fail grep -qF "could not be read" <<<"$OUT"

# any command after removing the config is equally silent
rm -rf "$XDG_CONFIG_HOME/dfm"
OUT=$(dfm paths 2>&1)
assert_fail grep -qF "could not be read" <<<"$OUT"

# a corrupt config is a real problem and must keep its warning
mkdir -p "$XDG_CONFIG_HOME/dfm"
echo 'garbage ===' > "$XDG_CONFIG_HOME/dfm/config.toml"
OUT=$(dfm paths 2>&1 || true)
assert_succ grep -qF "could not be read" <<<"$OUT"
