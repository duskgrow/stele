#!/usr/bin/env bash
# Wrapper to run rust-analyzer inside the flake devShell
exec nix develop --command rust-analyzer "$@"
