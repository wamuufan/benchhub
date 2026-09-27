#!/usr/bin/env python3
import sys
import os
import glob
import subprocess
import re
import json

def find_file(patterns):
    for pattern in patterns:
        matches = glob.glob(pattern, recursive=True)
        if matches:
            matches.sort()
            return matches[0]
    return None

def main():
    args = sys.argv[1:]

    # If the command is not 'benchmark', forward to the original Go binary if present
    if not args or args[0] != "benchmark":
        orig_bin = os.path.join(os.path.dirname(os.path.abspath(__file__)), "benchmark-launcher-cli-orig")
        if os.path.isfile(orig_bin):
            os.execv(orig_bin, [orig_bin] + args)
        else:
            print(f"Unknown command: {args}", file=sys.stderr)
            sys.exit(1)

    homes = []
    if os.environ.get("SNAP_REAL_HOME"):
        homes.append(os.environ.get("SNAP_REAL_HOME"))
    if os.environ.get("HOME"):
        homes.append(os.environ.get("HOME"))
    if os.environ.get("USER"):
        homes.append(f"/home/{os.environ.get('USER')}")
    seen = set()
    homes = [h for h in homes if h and not (h in seen or seen.add(h))]

    cwd = os.getcwd()
    script_dir = os.path.dirname(os.path.abspath(__file__))

    # Parse arguments
    scene_name = "monster"
    device_type = "CPU"

    # Search for scene name and device type
    for i, arg in enumerate(args):
        if arg in ["monster", "junkshop", "classroom"]:
            scene_name = arg
        elif arg == "--device-type" and i + 1 < len(args):
            device_type = args[i + 1]

    # If GPU mode environment variables are set and device_type wasn't explicitly overridden to something else
    if device_type == "CPU":
        nv_offload = os.environ.get("__NV_PRIME_RENDER_OFFLOAD", "0")
        if nv_offload == "1":
            device_type = "OPTIX"

    # 1. Locate Blender executable
    search_dirs = [cwd, script_dir]
    blender_patterns = [
        os.path.join(d, "blender") for d in search_dirs
    ] + [
        os.path.join(d, "blender-*/blender") for d in search_dirs
    ] + [
        os.path.join(d, "*/*/blender") for d in search_dirs
    ]
    for h in homes:
        blender_patterns.append(os.path.join(h, ".cache/blender-benchmark-launcher/blender-versions/*/*/blender"))
        blender_patterns.append(os.path.join(h, ".local/share/benchhub/runners/blender/*/*/blender"))

    blender_bin = find_file(blender_patterns)
    if not blender_bin:
        import shutil
        blender_bin = shutil.which("blender")

    if not blender_bin or not os.path.isfile(blender_bin):
        print("ERROR: Blender executable could not be found.", file=sys.stderr)
        sys.exit(1)

    # 2. Locate scene file
    scene_patterns = [
        os.path.join(d, f"{scene_name}/main.blend") for d in search_dirs
    ] + [
        os.path.join(d, f"scenes/*/{scene_name}/main.blend") for d in search_dirs
    ]
    for h in homes:
        scene_patterns.append(os.path.join(h, f".cache/blender-benchmark-launcher/scenes/*/{scene_name}/main.blend"))
        scene_patterns.append(os.path.join(h, f".local/share/benchhub/runners/blender/*/*/{scene_name}/main.blend"))

    scene_blend = find_file(scene_patterns)
    if not scene_blend or not os.path.isfile(scene_blend):
        print(f"ERROR: Blender scene '{scene_name}' could not be found.", file=sys.stderr)
        sys.exit(1)

    # 3. Locate benchmark script
    script_patterns = [
        os.path.join(d, "main.py") for d in search_dirs
    ] + [
        os.path.join(d, "blender_benchmark_script*/main.py") for d in search_dirs
    ] + [
        os.path.join(d, "benchmark-scripts/*/*/main.py") for d in search_dirs
    ]
    for h in homes:
        script_patterns.append(os.path.join(h, ".cache/blender-benchmark-launcher/benchmark-scripts/*/*/main.py"))
        script_patterns.append(os.path.join(h, ".local/share/benchhub/runners/blender/*/*/main.py"))

    script_main = find_file(script_patterns)
    if not script_main or not os.path.isfile(script_main):
        print("ERROR: Blender benchmark script (main.py) could not be found.", file=sys.stderr)
        sys.exit(1)

    cmd = [
        blender_bin,
        "--background",
        "--factory-startup",
        "-noaudio",
        "--debug-cycles",
        "--enable-autoexec",
        "--engine", "CYCLES",
        scene_blend,
        "--python", script_main,
        "--log", "render",
        "--",
        "--device-type", device_type
    ]

    print(f"[BenchHub Blender] Executable: {blender_bin}")
    print(f"[BenchHub Blender] Scene: {scene_blend}")
    print(f"[BenchHub Blender] Device Type: {device_type}")
    sys.stdout.flush()

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1
    )

    rendered_samples = None
    time_for_samples = None
    avg_time_per_sample = None
    render_time_no_sync = None
    peak_mem = None

    re_samples = re.compile(r"Rendered\s+(\d+)\s+samples\s+in\s+([0-9.]+)\s+seconds")
    re_avg = re.compile(r"Average\s+time\s+per\s+sample:\s+([0-9.]+)\s+seconds")
    re_no_sync = re.compile(r"Render\s+time\s+\(without\s+synchronization\):\s+([0-9.]+)")
    re_mem = re.compile(r"Peak:([0-9.]+[KM]?)")

    for line in iter(proc.stdout.readline, ''):
        print(line, end='')
        sys.stdout.flush()

        m = re_samples.search(line)
        if m:
            rendered_samples = int(m.group(1))
            time_for_samples = float(m.group(2))

        m = re_avg.search(line)
        if m:
            avg_time_per_sample = float(m.group(1))

        m = re_no_sync.search(line)
        if m:
            render_time_no_sync = float(m.group(1))

        m = re_mem.search(line)
        if m:
            peak_mem = m.group(1)

    proc.stdout.close()
    return_code = proc.wait()

    if return_code != 0:
        print(f"[BenchHub Blender] Blender exited with code {return_code}", file=sys.stderr)
        sys.exit(return_code)

    spm = None
    if rendered_samples and time_for_samples and time_for_samples > 0:
        spm = (rendered_samples / time_for_samples) * 60.0
    elif avg_time_per_sample and avg_time_per_sample > 0:
        spm = 60.0 / avg_time_per_sample

    if spm is not None:
        result = [
            {
                "stats": {
                    "samples_per_minute": spm,
                    "number_of_samples": rendered_samples,
                    "time_for_samples": time_for_samples,
                    "render_time_no_sync": render_time_no_sync,
                    "peak_memory": peak_mem
                }
            }
        ]
        print(json.dumps(result, indent=2))
        sys.stdout.flush()
    else:
        print("[BenchHub Blender] WARNING: Could not parse samples_per_minute from Cycles output.", file=sys.stderr)

if __name__ == "__main__":
    main()
