#!/usr/bin/env bash
# Capture stacks of a frozen neoism (and its embedded nvim) for diagnosis.
# Usage:  sudo scripts/freeze-dump.sh            # auto-detects pids
#         sudo scripts/freeze-dump.sh <neoism-pid>
# Output: /tmp/neoism-freeze-<timestamp>.txt  — attach/paste it to Claude.
set -u
out="/tmp/neoism-freeze-$(date +%H%M%S).txt"
pid="${1:-$(pgrep -x neoism | head -1)}"
if [ -z "${pid:-}" ]; then
    echo "no running neoism process found" >&2
    exit 1
fi
{
    echo "### neoism pid $pid  $(date)"
    echo "### thread states"
    ps -Lo tid,stat,pcpu,wchan:32,comm --pid "$pid"
    for i in 1 2 3; do
        echo "### neoism stack sample $i"
        eu-stack -p "$pid"
        sleep 0.7
    done
    for nv in $(pgrep -x nvim); do
        echo "### nvim pid $nv  cmdline: $(tr '\0' ' ' </proc/$nv/cmdline)"
        ps -Lo tid,stat,wchan:32 --pid "$nv"
        eu-stack -p "$nv"
        echo "### nvim fds"
        ls -l "/proc/$nv/fd/" 2>/dev/null
    done
    echo "### neoism fds"
    ls -l "/proc/$pid/fd/" 2>/dev/null
    echo "### nvim rpc pipe buffer sizes"
    # Find the nvim RPC pipes by cross-referencing neoism and nvim fd lists
    for nv in $(pgrep -x nvim); do
        # nvim reads RPC input on fd 14 (pipe from neoism) and writes responses on fd 15
        nvim_rpc_read=$(readlink "/proc/$nv/fd/14" 2>/dev/null | grep -o '\[[0-9]*\]' | tr -d '[]')
        nvim_rpc_write=$(readlink "/proc/$nv/fd/15" 2>/dev/null | grep -o '\[[0-9]*\]' | tr -d '[]')
        echo "nvim pid=$nv rpc_in_pipe=$nvim_rpc_read rpc_out_pipe=$nvim_rpc_write"
        for fd_pair in "14:$nvim_rpc_read" "15:$nvim_rpc_write"; do
            fd="${fd_pair%%:*}"; ino="${fd_pair##*:}"
            echo "  nvim fd $fd (pipe $ino) fdinfo:"
            cat "/proc/$nv/fdinfo/$fd" 2>/dev/null
        done
        # Find neoism's side of same pipes and check fdinfo
        for fd in $(ls "/proc/$pid/fd/" 2>/dev/null); do
            target=$(readlink "/proc/$pid/fd/$fd" 2>/dev/null)
            if echo "$target" | grep -qE "\[($nvim_rpc_read|$nvim_rpc_write)\]"; then
                echo "  neoism fd $fd -> $target fdinfo:"
                cat "/proc/$pid/fdinfo/$fd" 2>/dev/null
            fi
        done
    done
    echo "### tokio task count estimate (neoism-embedded thread wchans)"
    for tid in $(ls "/proc/$pid/task/" 2>/dev/null); do
        comm=$(cat "/proc/$pid/task/$tid/comm" 2>/dev/null | tr -d '\n')
        wchan=$(cat "/proc/$pid/task/$tid/wchan" 2>/dev/null)
        syscall=$(cat "/proc/$pid/task/$tid/syscall" 2>/dev/null | head -1)
        if echo "$comm" | grep -q "embedded\|desktop"; then
            echo "  tid=$tid comm=$comm wchan=$wchan syscall_nr=${syscall%% *}"
        fi
    done
} >"$out" 2>&1
echo "wrote $out"
