#!/bin/bash
for git in */.git; do ( echo "• "$(dirname "$git")":"; cd "$(dirname "$git")" && "$@" ); done
