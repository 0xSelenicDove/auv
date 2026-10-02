import json
import os
import sys
from datetime import datetime, timezone

LOG_FILE = os.environ.get("EVAL_LOG_FILE", r"F:\auv\docs\ai\references\3dgs\eval_data\vlm_raw_io.jsonl")

def log_event(event_obj):
    os.makedirs(os.path.dirname(LOG_FILE), exist_ok=True)
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(json.dumps(event_obj, ensure_ascii=False) + "\n")

def estimate_tokens(text: str) -> int:
    cjk_count = sum(1 for c in text if '\u4e00' <= c <= '\u9fff')
    non_cjk = len(text) - cjk_count
    return max(1, int(cjk_count / 1.5 + non_cjk / 4.0))

def main():
    if len(sys.argv) < 3:
        print("Usage: python eval_vlm_cli.py log_answer <qid> '<json_payload>'")
        sys.exit(1)

    cmd = sys.argv[1]
    qid = sys.argv[2]
    now_iso = datetime.now(timezone.utc).isoformat()

    if cmd == "log_answer":
        raw_payload = sys.argv[3]
        if raw_payload == "--file" and len(sys.argv) > 4:
            with open(sys.argv[4], "r", encoding="utf-8") as f:
                payload = json.load(f)
        else:
            payload = json.loads(raw_payload)
        
        prompt_text = payload.get("prompt_text", "")
        reasoning = payload.get("reasoning", "")
        final_answer = payload.get("final_answer_text", "")
        visual_desc = payload.get("visual_description", "")
        
        prompt_tok = estimate_tokens(prompt_text)
        comp_tok = estimate_tokens(visual_desc + reasoning + final_answer)
        image_tok = 800 # Fixed multimodal token allocation for 854x480 frame

        event = {
            "timestamp": now_iso,
            "question_id": qid,
            "event_type": "vlm_answer",
            "image_path": payload.get("image_path", ""),
            "prompt_text": prompt_text,
            "visual_description": visual_desc,
            "reasoning": reasoning,
            "bearing": payload.get("bearing"),
            "distance_m": payload.get("distance_m"),
            "visible_landmarks": payload.get("visible_landmarks", []),
            "has_target_behind": payload.get("has_target_behind"),
            "raw_answer_text": final_answer,
            "tokens": {
                "image_tokens_est": image_tok,
                "prompt_tokens_est": prompt_tok,
                "completion_tokens_est": comp_tok,
                "total_tokens_est": image_tok + prompt_tok + comp_tok
            }
        }
        log_event(event)
        print(f"Logged VLM answer for {qid}")

if __name__ == "__main__":
    main()
