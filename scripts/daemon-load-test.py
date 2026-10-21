#!/usr/bin/env python3
"""
daemon-load-test.py - High-throughput concurrent load generator for cargo-canon daemon.

Sends concurrent PostReadHook, Slice, Ping, and Status IPC requests over the Unix domain
socket, measuring request latency, throughput (req/s), and context bloat reduction.
"""

import argparse
import glob
import json
import os
import random
import socket
import struct
import sys
import threading
import time


def resolve_default_socket():
    # 1. Check XDG_RUNTIME_DIR
    xdg_runtime = os.environ.get("XDG_RUNTIME_DIR")
    if xdg_runtime:
        sock = os.path.join(xdg_runtime, "canon", "daemon.sock")
        if os.path.exists(sock):
            return sock
    # 2. Check /run/user/<uid>/canon/daemon.sock
    uid_sock = f"/run/user/{os.getuid()}/canon/daemon.sock"
    if os.path.exists(uid_sock):
        return uid_sock
    # 3. Check /tmp/canon-*.sock
    tmp_socks = glob.glob("/tmp/canon-*.sock")
    if tmp_socks:
        return tmp_socks[0]
    return uid_sock


def send_ipc_message(sock, payload_dict):
    data = json.dumps(payload_dict).encode("utf-8")
    sock.sendall(struct.pack(">I", len(data)) + data)
    resp_len = struct.unpack(">I", sock.recv(4))[0]
    chunks = []
    received = 0
    while received < resp_len:
        chunk = sock.recv(min(8192, resp_len - received))
        if not chunk:
            raise ConnectionResetError("Daemon closed connection prematurely")
        chunks.append(chunk)
        received += len(chunk)
    return json.loads(b"".join(chunks))


def compute_line_offsets(path):
    try:
        with open(path, "rb") as f:
            lines = f.read().splitlines(keepends=True)
    except OSError:
        return [0]
    offsets = [0]
    for line in lines:
        offsets.append(offsets[-1] + len(line))
    return offsets


def build_post_read_hook(target_file, offsets, rng, req_count):
    abs_path = os.path.abspath(target_file)
    total_lines = len(offsets) - 1
    # Periodically omit line ranges to exercise daemon/vendor default handling
    if total_lines <= 0 or req_count % 5 == 0:
        return {"PostReadHook": {"file": abs_path}}

    start_line = rng.randint(1, total_lines)
    if req_count % 2 == 1:
        # Micro-read window (up to 80 lines)
        end_line = min(total_lines, start_line + rng.randint(0, 79))
    else:
        # Arbitrary valid span up to end of file
        end_line = rng.randint(start_line, total_lines)

    raw_read_bytes = offsets[end_line] - offsets[start_line - 1]
    return {
        "PostReadHook": {
            "file": abs_path,
            "start_line": start_line,
            "end_line": end_line,
            "raw_read_bytes": raw_read_bytes,
        }
    }


def worker_loop(sock_path, files, file_offsets, mode, duration_secs, max_requests, stats, seed=42):
    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.connect(sock_path)
    except Exception:
        stats["errors"] += 1
        return

    rng = random.Random(seed)
    start_time = time.monotonic()
    req_count = 0
    file_idx = 0
    num_files = len(files)

    while True:
        if duration_secs > 0 and (time.monotonic() - start_time) >= duration_secs:
            break
        if max_requests > 0 and req_count >= max_requests:
            break

        target_file = files[file_idx % num_files] if num_files > 0 else ""
        file_idx += 1
        offsets = file_offsets.get(target_file, [0])

        if mode == "hook":
            req = build_post_read_hook(target_file, offsets, rng, req_count)
        elif mode == "slice":
            req = {
                "Slice": {
                    "file": os.path.abspath(target_file),
                    "mode": "skeleton",
                    "format": "markdown",
                }
            }
        elif mode == "mixed":
            op = req_count % 3
            if op == 0:
                req = build_post_read_hook(target_file, offsets, rng, req_count)
            elif op == 1:
                req = {
                    "Slice": {
                        "file": os.path.abspath(target_file),
                        "mode": "skeleton",
                        "format": "markdown",
                    }
                }
            else:
                req = "Ping"
        else:
            req = "Ping"

        t0 = time.monotonic()
        try:
            resp = send_ipc_message(sock, req)
            lat = (time.monotonic() - t0) * 1000.0  # ms
            stats["latencies"].append(lat)
            stats["success"] += 1
        except Exception:
            stats["errors"] += 1
            break

        req_count += 1

    sock.close()


def discover_source_files(root_dirs):
    extensions = {".rs", ".ts", ".tsx", ".js", ".jsx", ".go"}
    files = []
    repo_breakdown = {}
    for root_dir in root_dirs:
        abs_root = os.path.abspath(root_dir)
        repo_files = []
        for dirpath, dirnames, filenames in os.walk(abs_root):
            # Skip common non-source directories
            dirnames[:] = [
                d for d in dirnames
                if d not in {".git", "target", "node_modules", "dist", "build", ".claude", ".cursor"}
            ]
            for fname in filenames:
                ext = os.path.splitext(fname)[1]
                if ext in extensions:
                    full_p = os.path.join(dirpath, fname)
                    repo_files.append(full_p)
        repo_breakdown[abs_root] = len(repo_files)
        files.extend(repo_files)
    return files, repo_breakdown


def main():
    parser = argparse.ArgumentParser(
        description="Generate concurrent load against the canon indexing daemon."
    )
    parser.add_argument(
        "--socket",
        help="Path to daemon Unix domain socket (default: auto-detected)",
    )
    parser.add_argument(
        "--threads",
        type=int,
        default=8,
        help="Number of concurrent worker threads (default: 8)",
    )
    parser.add_argument(
        "--duration",
        type=float,
        default=5.0,
        help="Duration to run load test in seconds (default: 5.0, 0 = unlimited)",
    )
    parser.add_argument(
        "--requests",
        type=int,
        default=0,
        help="Total requests per thread (default: 0 = duration based)",
    )
    parser.add_argument(
        "--mode",
        choices=["hook", "slice", "ping", "mixed"],
        default="hook",
        help="Request payload type (default: hook)",
    )
    parser.add_argument(
        "--target-dirs",
        "--target-dir",
        nargs="+",
        default=["."],
        dest="target_dirs",
        help="One or more directory paths (git repos) to gather source files from (default: .)",
    )

    args = parser.parse_args()

    sock_path = args.socket or resolve_default_socket()
    if not os.path.exists(sock_path):
        print(f"Error: Daemon socket not found at '{sock_path}'. Is 'canon daemon' running?", file=sys.stderr)
        sys.exit(1)

    files, breakdown = discover_source_files(args.target_dirs)
    if not files and args.mode in ("hook", "slice", "mixed"):
        print(f"Warning: No matching source files (.rs, .ts, .go) found in {args.target_dirs}.", file=sys.stderr)

    # Shuffle to interleave requests across different repositories and directories
    random.seed(42)
    random.shuffle(files)
    file_offsets = {f: compute_line_offsets(f) for f in files}

    print(f"=== Canon Daemon Multi-Tenant Load Test ===")
    print(f"Socket:        {sock_path}")
    print(f"Concurrency:   {args.threads} threads")
    print(f"Mode:          {args.mode}")
    print(f"Target Repos:  {len(args.target_dirs)} root(s)")
    for repo, count in breakdown.items():
        print(f"  - {repo}: {count} source files")
    print(f"Total Files:   {len(files)} files discovered")
    print(f"Duration:      {args.duration}s" if args.duration > 0 else f"Requests/thr: {args.requests}")
    print("Generating load across registered workspaces...")

    worker_stats = [{"latencies": [], "success": 0, "errors": 0} for _ in range(args.threads)]

    t_start = time.monotonic()
    threads = []
    for i in range(args.threads):
        t = threading.Thread(
            target=worker_loop,
            args=(
                sock_path,
                files,
                file_offsets,
                args.mode,
                args.duration,
                args.requests,
                worker_stats[i],
                42 + i,
            ),
        )
        threads.append(t)
        t.start()

    for t in threads:
        t.join()

    total_time = time.monotonic() - t_start

    all_lats = []
    total_success = 0
    total_errors = 0
    for st in worker_stats:
        all_lats.extend(st["latencies"])
        total_success += st["success"]
        total_errors += st["errors"]

    all_lats.sort()
    rps = total_success / total_time if total_time > 0 else 0

    print("\n=== Load Test Results ===")
    print(f"Elapsed Time:      {total_time:.2f} s")
    print(f"Total Requests:    {total_success}")
    print(f"Failed Requests:   {total_errors}")
    print(f"Throughput:        {rps:.1f} req/s")

    if all_lats:
        p50 = all_lats[int(len(all_lats) * 0.50)]
        p90 = all_lats[int(len(all_lats) * 0.90)]
        p95 = all_lats[int(len(all_lats) * 0.95)]
        p99 = all_lats[int(len(all_lats) * 0.99)]
        avg = sum(all_lats) / len(all_lats)
        print(f"Latency Avg:       {avg:.2f} ms")
        print(f"Latency p50:       {p50:.2f} ms")
        print(f"Latency p90:       {p90:.2f} ms")
        print(f"Latency p95:       {p95:.2f} ms")
        print(f"Latency p99:       {p99:.2f} ms")


if __name__ == "__main__":
    main()
