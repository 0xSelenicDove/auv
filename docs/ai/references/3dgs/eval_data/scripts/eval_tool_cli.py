import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone

LOG_FILE = os.environ.get("EVAL_LOG_FILE", r"F:\auv\docs\ai\references\3dgs\eval_data\tool_raw_io.jsonl")
STORE_FILE = r"F:\auv\.tmp\memory_eval_spike\eval_store.json"
HARNESS_EXE = r"F:\auv\target\debug\memory_eval_harness.exe"

def log_event(event_obj):
    os.makedirs(os.path.dirname(LOG_FILE), exist_ok=True)
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(json.dumps(event_obj, ensure_ascii=False) + "\n")

def estimate_tokens(text: str) -> int:
    # Character / word based token estimation: ~4 chars per token for English, ~1.5 per char for CJK
    cjk_count = sum(1 for c in text if '\u4e00' <= c <= '\u9fff')
    non_cjk = len(text) - cjk_count
    return max(1, int(cjk_count / 1.5 + non_cjk / 4.0))

def main():
    if len(sys.argv) < 3:
        print("Usage:")
        print("  python eval_tool_cli.py search <qid> <label>")
        print("  python eval_tool_cli.py query <qid> <target> <kind> <x> <y> <z> <yaw> <pitch> [fov]")
        print("  python eval_tool_cli.py get <qid> <target>")
        print("  python eval_tool_cli.py log_answer <qid> '<json_answer>'")
        sys.exit(1)

    cmd = sys.argv[1]
    qid = sys.argv[2]
    now_iso = datetime.now(timezone.utc).isoformat()

    if cmd == "search":
        label = sys.argv[3]
        input_data = {"label": label}
        input_json = json.dumps(input_data)
        t0 = time.time()
        proc = subprocess.run(
            [HARNESS_EXE, "--tool-exec", "--store", STORE_FILE, "--tool", "memory_search", "--input", input_json],
            capture_output=True, text=True, encoding="utf-8"
        )
        latency_ms = int((time.time() - t0) * 1000)
        if proc.returncode == 0:
            res = proc.stdout
            try:
                parsed_res = json.loads(res)
            except Exception:
                parsed_res = {"raw": res}
        else:
            res = (proc.stderr or proc.stdout).strip()
            parsed_res = {"error": res}
        
        event = {
            "timestamp": now_iso,
            "question_id": qid,
            "event_type": "tool_call",
            "tool_name": "memory_search",
            "input_args": input_data,
            "raw_output": parsed_res,
            "latency_ms": latency_ms,
            "tool_tokens_est": estimate_tokens(input_json + res)
        }
        log_event(event)
        print(res)

    elif cmd == "query":
        target = sys.argv[3]
        kind = sys.argv[4]
        x, y, z = float(sys.argv[5]), float(sys.argv[6]), float(sys.argv[7])
        yaw = float(sys.argv[8])
        pitch = float(sys.argv[9])
        fov = float(sys.argv[10]) if len(sys.argv) > 10 else 70.0

        target_field = {"landmark_id": target} if target.startswith("lm-") else {"block_pos": [int(v) for v in target.split(",")]}
        input_data = {
            "target": target_field,
            "query_kind": kind,
            "observer_pose": {
                "eye_position": {"x": x, "y": y, "z": z},
                "yaw": yaw,
                "pitch": pitch,
            },
            "viewport": {"width": 854, "height": 480},
            "vertical_fov_deg": fov,
            "now_millis": 2453830
        }
        input_json = json.dumps(input_data)
        t0 = time.time()
        proc = subprocess.run(
            [HARNESS_EXE, "--tool-exec", "--store", STORE_FILE, "--tool", "memory_query", "--input", input_json],
            capture_output=True, text=True, encoding="utf-8"
        )
        latency_ms = int((time.time() - t0) * 1000)
        if proc.returncode == 0:
            res = proc.stdout
            try:
                parsed_res = json.loads(res)
            except Exception:
                parsed_res = {"raw": res}
        else:
            res = (proc.stderr or proc.stdout).strip()
            parsed_res = {"error": res}

        event = {
            "timestamp": now_iso,
            "question_id": qid,
            "event_type": "tool_call",
            "tool_name": "memory_query",
            "input_args": input_data,
            "raw_output": parsed_res,
            "latency_ms": latency_ms,
            "tool_tokens_est": estimate_tokens(input_json + res)
        }
        log_event(event)
        print(res)

    elif cmd == "get":
        target = sys.argv[3]
        input_data = {"landmark_id": target}
        input_json = json.dumps(input_data)
        t0 = time.time()
        proc = subprocess.run(
            [HARNESS_EXE, "--tool-exec", "--store", STORE_FILE, "--tool", "memory_get", "--input", input_json],
            capture_output=True, text=True, encoding="utf-8"
        )
        latency_ms = int((time.time() - t0) * 1000)
        if proc.returncode == 0:
            res = proc.stdout
            try:
                parsed_res = json.loads(res)
            except Exception:
                parsed_res = {"raw": res}
        else:
            res = (proc.stderr or proc.stdout).strip()
            parsed_res = {"error": res}

        event = {
            "timestamp": now_iso,
            "question_id": qid,
            "event_type": "tool_call",
            "tool_name": "memory_get",
            "input_args": input_data,
            "raw_output": parsed_res,
            "latency_ms": latency_ms,
            "tool_tokens_est": estimate_tokens(input_json + res)
        }
        log_event(event)
        print(res)

    elif cmd == "log_answer":
        raw_ans = sys.argv[3]
        if raw_ans == "--file" and len(sys.argv) > 4:
            with open(sys.argv[4], "r", encoding="utf-8") as f:
                parsed_ans = json.load(f)
        else:
            parsed_ans = json.loads(raw_ans)
        prompt_tokens = estimate_tokens(parsed_ans.get("prompt_text", ""))
        completion_tokens = estimate_tokens(parsed_ans.get("reasoning", "") + parsed_ans.get("final_answer_text", ""))

        event = {
            "timestamp": now_iso,
            "question_id": qid,
            "event_type": "final_answer",
            "answer_payload": parsed_ans,
            "tokens": {
                "prompt_tokens_est": prompt_tokens,
                "completion_tokens_est": completion_tokens,
                "tool_tokens_est": parsed_ans.get("tool_tokens_est", 0),
                "total_tokens_est": prompt_tokens + completion_tokens + parsed_ans.get("tool_tokens_est", 0)
            }
        }
        log_event(event)
        print(f"Logged answer for {qid}")

if __name__ == "__main__":
    main()
