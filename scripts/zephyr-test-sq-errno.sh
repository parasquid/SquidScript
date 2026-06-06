#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"

EXTRA_ARGS=()
while [[ $# -gt 0 ]]; do
	case "$1" in
		--help|-h)
			echo "Usage: scripts/zephyr-test-sq-errno.sh [-- <extra twister args>]"
			echo "Runs the sq_errno code->name decode ztests on native_sim/native/64."
			exit 0
			;;
		--)
			shift
			EXTRA_ARGS=("$@")
			break
			;;
		*)
			EXTRA_ARGS+=("$1")
			;;
	esac
	shift
done

west twister \
	-T "${ROOT}/firmware/zephyr/tests/sq-errno" \
	--platform native_sim/native/64 \
	--inline-logs \
	--clobber-output \
	"${EXTRA_ARGS[@]}"
