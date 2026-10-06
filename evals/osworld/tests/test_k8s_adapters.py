"""Public manifest contracts for the task-specific Kubernetes adapters."""

import json
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from auv_osworld import k8s_phase_adapter as cluster
from auv_osworld import k8s_typed_action_adapter as typed
from auv_osworld import k8s_v2_task044_adapter as task044
from auv_osworld import k8s_v2_task099_adapter as task099


def common_config(root: Path, *, task_id: str | None = None) -> dict:
    root.mkdir(parents=True, exist_ok=True)
    files = {}
    for name in ("kubeconfig", "guest_auv_binary", "host_auv_binary"):
        file = root / name
        file.write_bytes(name.encode())
        files[name] = str(file)
    upstream = root / "upstream"
    upstream.mkdir(exist_ok=True)
    config = {
        "batch_id": "batch",
        "episode_id": f"episode-{task_id or 'v2'}",
        "namespace": "bench",
        "kubeconfig": files["kubeconfig"],
        "context": "test",
        "node": "worker",
        "runtime_pod": f"vm-{task_id or 'v2'}",
        "runtime_service": f"service-{task_id or 'v2'}",
        "proxy_pod": f"proxy-{task_id or 'v2'}",
        "proxy_image": "example/proxy@sha256:" + "a" * 64,
        "base_pvc": "osworld-v1-hot",
        "base_qcow_sha256": "b" * 64,
        "guest_auv_binary": files["guest_auv_binary"],
        "host_auv_binary": files["host_auv_binary"],
        "upstream_checkout": str(upstream),
        "setup_local_port": 25000,
        "auv_local_port": 25001,
    }
    if task_id:
        config["task_id"] = task_id
    return config


@pytest.mark.parametrize(
    ("adapter", "task_id", "extra"),
    [
        (task044, "044", {}),
        (
            task099,
            "099",
            {
                "action_binary_sha256": "c" * 64,
                "action_source_commit": "d" * 40,
            },
        ),
    ],
)
def test_v2_manifest_exposes_one_six_phase_episode(tmp_path, adapter, task_id, extra):
    config = common_config(tmp_path)
    config.update(
        {
            "base_pvc": adapter.V2_HOT_PVC,
            "base_qcow_sha256": adapter.V2_BASE_QCOW_SHA256,
            "host_auv_sha256": "b" * 64,
            "task_source": str(tmp_path / f"task_{task_id}.py"),
            "asset": str(tmp_path / "asset"),
            **extra,
        }
    )
    Path(config["task_source"]).write_text("# fixture\n")
    Path(config["asset"]).write_bytes(b"asset")
    if adapter is task099:
        action = tmp_path / "auv-osworld-action"
        action.write_bytes(b"action")
        action.chmod(0o700)
        config["action_binary"] = str(action)
    config_path = tmp_path / "config.json"
    config_path.write_text(json.dumps(config))

    digests = {
        config["guest_auv_binary"]: adapter.GUEST_AUV_SHA256,
        config["host_auv_binary"]: config["host_auv_sha256"],
        str(config_path.resolve()): "e" * 64,
    }
    if adapter is task099:
        digests[config["action_binary"]] = config["action_binary_sha256"]
    evaluator = adapter.task044 if adapter is task044 else adapter.task099
    with (
        patch.object(cluster, "sha256", side_effect=lambda path: digests[str(path)]),
        patch.object(evaluator, "load_task"),
        patch.object(adapter.subprocess, "run", return_value=MagicMock(returncode=0)),
    ):
        manifest = adapter.manifest(config_path)

    episode = manifest["episodes"][0]
    assert manifest["batch_id"] == config["batch_id"]
    assert episode["identity"]["task_id"] == task_id
    assert set(episode["phases"]) == set(cluster.PHASES)
    assert all(phase["argv"][3:5] == ["phase", name] for name, phase in episode["phases"].items())

    config["action_argv"] = ["xdotool", "click", "1"]
    config_path.write_text(json.dumps(config))
    with pytest.raises(ValueError):
        adapter.load_config(config_path)


def test_typed_v1_manifest_keeps_exact_task_set_and_static_actions(tmp_path):
    task_ids = [cluster.CHROME_TASK, cluster.VLC_TASK]
    configs = []
    for index, task_id in enumerate(task_ids):
        config = common_config(tmp_path / f"config-{index}", task_id=task_id)
        config["setup_local_port"] += index * 2
        config["auv_local_port"] += index * 2
        path = tmp_path / f"episode-{index}.json"
        path.write_text(json.dumps(config))
        configs.append(path)
    binary = tmp_path / "auv-osworld-action"
    binary.write_bytes(b"action")
    batch = tmp_path / "batch.json"
    batch.write_text(
        json.dumps({"batch_id": "batch", "episodes": [str(path) for path in configs], "action_binary": str(binary)})
    )

    def capture_manifest(path):
        config = json.loads(Path(path).read_text())
        return {
            "trust": "operator-audited",
            "batch_id": "batch",
            "episodes": [
                {
                    "episode_id": config["episode_id"],
                    "identity": {"task_id": config["task_id"]},
                    "phases": {
                        name: {"argv": ["python", "adapter", "phase", name], "timeout_seconds": cluster.TIMEOUTS[name]}
                        for name in cluster.PHASES
                    },
                }
            ],
        }

    with (
        patch.object(typed.capture, "manifest", side_effect=capture_manifest),
        patch.object(typed.capture, "sha256", return_value=typed.ACTION_SHA256),
    ):
        manifest = typed.manifest(batch)

    assert [episode["identity"]["task_id"] for episode in manifest["episodes"]] == task_ids
    assert typed.template(task_ids[0])["actions"][-1] == "DONE"
    assert typed.template(task_ids[1])["actions"][-1] == "DONE"
