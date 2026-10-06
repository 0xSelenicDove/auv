"""Kubernetes adapter contract tests. Every cluster/process boundary is mocked."""

import importlib.util
from contextlib import nullcontext
import hashlib
import json
import os
from pathlib import Path
import socket
import tempfile
import unittest
from unittest.mock import MagicMock, patch


ADAPTER_PATH = Path(__file__).resolve().parents[1] / "k8s_phase_adapter.py"
spec = importlib.util.spec_from_file_location("k8s_phase_adapter", ADAPTER_PATH)
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


# Frozen from the 2026-10-06 V1 live boot that the old -snapshot assertion rejected.
LIVE_V1_QEMU_ARGV = (
    "qemu-system-x86_64 -cpu host,kvm=on,l3-cache=on,+hypervisor,migratable=no,+invtsc "
    "-smp 4,sockets=1,dies=1,cores=4,threads=1 -m 8G "
    "-machine type=q35,smm=off,graphics=off,vmport=off,dump-guest-core=off,hpet=off,accel=kvm "
    "-enable-kvm -global kvm-pit.lost_tick_policy=discard -display vnc=:0,websocket=5700 "
    "-vga virtio -monitor telnet:localhost:7100,server,nowait,nodelay "
    "-name qemu,process=qemu,debug-threads=on -serial mon:stdio "
    "-device qemu-xhci,id=xhci -device usb-tablet "
    "-netdev tap,id=hostnet0,ifname=qemu,vhost=on,vhostfd=40,script=no,downscript=no "
    "-device virtio-net-pci,romfile=,netdev=hostnet0,mac=02:E4:1B:C3:6B:DF,id=net0 "
    "-hda /boot.qcow2 -pflash /storage/uefi.rom "
    "-object rng-random,id=objrng0,filename=/dev/urandom "
    "-device virtio-rng-pci,rng=objrng0,id=rng0,bus=pcie.0,addr=0x1c "
    "-device virtio-balloon-pci,id=balloon0,bus=pcie.0,addr=0x4"
)


def config() -> dict:
    return {
        "batch_id": "control-1", "episode_id": "chrome-1", "namespace": "bench",
        "kubeconfig": "/fake/kubeconfig", "context": "test-context", "node": "liet-gpu-1",
        "runtime_pod": "chrome-vm", "runtime_service": "chrome-svc", "proxy_pod": "chrome-proxy",
        "proxy_image": "registry.example/proxy@sha256:" + "a" * 64,
        "base_pvc": "osworld-v1-hot", "base_qcow_sha256": "b" * 64,
        "guest_auv_binary": "/fake/guest-auv", "host_auv_binary": "/fake/host-auv",
        "upstream_checkout": "/fake/osworld", "setup_local_port": 25000, "auv_local_port": 28080,
    }


class AdapterTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name) / "chrome-1"
        self.directory.mkdir()
        self.episode = adapter.Episode(config(), self.directory)

    def test_manifest_has_no_configurable_action_argv_and_is_capture_only(self):
        with patch.object(adapter, "load_config", return_value=config()):
            with patch.object(Path, "resolve", return_value=Path("/fake/config.json")):
                built = adapter.manifest(Path("/fake/config.json"))
        episode = built["episodes"][0]
        self.assertEqual(episode["identity"]["topology"], "paired-remote-capture-only-negative-control")
        self.assertEqual(set(episode["phases"]), set(adapter.PHASES))
        self.assertEqual(episode["phases"]["action"]["argv"][2:4], ["phase", "action"])
        self.assertEqual(episode["phases"]["action"]["timeout_seconds"], 600)
        self.assertNotIn("action_argv", config())

    def test_config_rejects_missing_measured_qcow_and_any_action_argv(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "config.json"
            value = config()
            value.pop("base_qcow_sha256")
            path.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, "exactly"):
                adapter.load_config(path)

    def test_config_validates_pinned_source_and_binary_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            value = config()
            for field in ("kubeconfig", "guest_auv_binary", "host_auv_binary"):
                target = root / field
                target.write_bytes(field.encode())
                value[field] = str(target)
            upstream = root / "OSWorld"
            task = upstream / "evaluation_examples/examples/chrome" / f"{adapter.CHROME_TASK}.json"
            task.parent.mkdir(parents=True)
            task.write_text("{}")
            value["upstream_checkout"] = str(upstream)
            path = root / "config.json"
            path.write_text(json.dumps(value))
            with patch.object(adapter, "sha256", side_effect=lambda candidate: {
                value["guest_auv_binary"]: adapter.GUEST_AUV_SHA256,
                value["host_auv_binary"]: adapter.HOST_AUV_SHA256,
                str(task): adapter.CHROME_SHA256,
            }[str(candidate)]), patch.object(adapter.subprocess, "run") as run:
                run.side_effect = [MagicMock(stdout=adapter.V1_REVISION), MagicMock(stdout="")]
                self.assertEqual(adapter.load_config(path), value)
            value["base_qcow_sha256"] = "unknown"
            path.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, "measured digest"):
                adapter.load_config(path)
            value = config()
            value["action_argv"] = ["sh", "-c", "xdotool click 1"]
            path.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, "exactly"):
                adapter.load_config(path)

    def test_guest_control_rejects_osworld_relay_of_auv_gui_input(self):
        with patch.object(self.episode, "_post", return_value={
            "status": "success", "output": "known", "error": "", "returncode": 0,
        }) as post:
            with self.assertRaisesRegex(ValueError, "forbidden"):
                self.episode.guest_control(["/home/user/auv", "invoke", "input.clickPoint", "10", "20"])
            post.assert_not_called()
            self.episode.guest_control(["sha256sum", "/home/user/auv"])
            post.assert_called_once_with("/setup/execute", {"command": ["sha256sum", "/home/user/auv"], "shell": False})

    def test_guest_control_rejects_nonzero_command_behind_http_200_without_leaking_output(self):
        # ROOT CAUSE:
        # OSWorld /setup/execute returns HTTP 200 and status=success even when
        # the guest command exits nonzero. Ignoring returncode hid the pairing
        # failure behind an empty token output.
        secret = "do-not-log-this-pairing-value"
        response = {"status": "success", "output": "", "error": secret, "returncode": 1}
        with patch.object(self.episode, "_post", return_value=response):
            with self.assertRaisesRegex(RuntimeError, "returncode=1") as caught:
                self.episode.guest_control(["env", "AUV_ENDPOINT=unix:///home/user/auv.sock", "/home/user/auv", "devices", "pair", "create-token"])
        self.assertNotIn(secret, str(caught.exception))
        self.assertIn("stderr_sha256=", str(caught.exception))

    def test_pair_token_shape_failure_reports_metadata_not_bearer(self):
        secret = "do-not-log-this-pairing-value"
        reply = {"status": "success", "output": f"warning\n{secret}\n", "error": "", "returncode": 0}
        good = lambda output: {"status": "success", "output": output, "error": "", "returncode": 0}
        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(adapter, "_run", return_value=""), \
             patch.object(self.episode, "guest_control", side_effect=[
                 good(""), good(adapter.GUEST_AUV_SHA256 + "  /home/user/auv"),
                 good(""), good(""), good("auv 0.0.28"), reply,
             ]), \
             patch.object(self.episode, "_post", return_value="launched successfully"):
            with self.assertRaisesRegex(ValueError, "stdout_lines=2") as caught:
                self.episode.install()
        self.assertNotIn(secret, str(caught.exception))
        self.assertIn("stdout_sha256=", str(caught.exception))

    def test_install_prepares_guest_libraries_before_auv_version(self):
        calls = []
        def control(command):
            calls.append(command)
            if command == ["sha256sum", "/home/user/auv"]:
                return {"output": adapter.GUEST_AUV_SHA256 + "  /home/user/auv"}
            if command == ["/home/user/auv", "--version"]:
                raise RuntimeError("stop after library preparation")
            return {"output": ""}
        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(adapter, "_run", return_value=""), \
             patch.object(self.episode, "guest_control", side_effect=control):
            with self.assertRaisesRegex(RuntimeError, "stop after library preparation"):
                self.episode.install()
        self.assertEqual(calls, [
            ["chmod", "0700", "/home/user/auv"],
            ["sha256sum", "/home/user/auv"],
            adapter.GUEST_APT_UPDATE,
            adapter.GUEST_APT_INSTALL,
            ["/home/user/auv", "--version"],
        ])

    def test_launch_accepts_pinned_upstream_plain_text_success(self):
        # ROOT CAUSE:
        # The pinned OSWorld /setup/launch returns plain text on HTTP 200.
        # Parsing it as JSON rejected a successful daemon launch before
        # pairing, so this response has a distinct explicit contract.
        response = MagicMock()
        response.status = 200
        response.read.return_value = b"/home/user/auv serve launched successfully"
        with patch.object(adapter.request, "urlopen") as urlopen:
            urlopen.return_value.__enter__.return_value = response
            result = self.episode._post("/setup/launch", {"command": ["/home/user/auv", "serve"], "shell": False})
        self.assertEqual(result, "/home/user/auv serve launched successfully")

    def test_launch_rejects_unconfirmed_text_while_execute_keeps_json_contract(self):
        response = MagicMock()
        response.status = 200
        with patch.object(adapter.request, "urlopen") as urlopen:
            urlopen.return_value.__enter__.return_value = response
            response.read.return_value = b"unexpected launch response"
            with self.assertRaisesRegex(ValueError, "no success confirmation"):
                self.episode._post("/setup/launch", {"command": ["/home/user/auv", "serve"], "shell": False})
            response.read.return_value = b'{"output":"known control response"}'
            result = self.episode._post("/setup/execute", {"command": ["sha256sum", "/home/user/auv"], "shell": False})
        self.assertEqual(result, {"output": "known control response"})

    def test_service_selects_runtime_not_proxy(self):
        created = []
        with patch.object(self.episode, "_retained_pvc", return_value={"name": "osworld-v1-hot"}), \
             patch.object(self.episode, "_create", side_effect=created.append), \
             patch.object(self.episode, "kubectl"), \
             patch.object(self.episode, "_pod_snapshot", side_effect=[{"uid": "vm"}, {"uid": "proxy"}]), \
             patch.object(self.episode, "get", return_value={"metadata": {"uid": "svc"}}), \
             patch.object(self.episode, "_overlay", return_value={"overlay": "qcow2 backing-file", "runtime": {"uid": "vm"}}), \
             patch.object(self.episode, "_stable_guest_control"), \
             patch.object(self.episode, "assert_identity"), \
             patch.object(adapter.time, "sleep"):
            self.episode.boot()
        pod, service, proxy = created
        self.assertEqual(service["spec"]["selector"], pod["metadata"]["labels"])
        self.assertNotEqual(service["spec"]["selector"], proxy["metadata"]["labels"])
        self.assertEqual(pod["spec"]["containers"][0]["startupProbe"]["tcpSocket"], {"port": 5000})
        self.assertEqual(pod["spec"]["volumes"][0]["persistentVolumeClaim"],
                         {"claimName": "osworld-v1-hot", "readOnly": True})
        self.assertNotIn("/screenshot", json.dumps(created))
        self.assertEqual(json.loads(self.episode.identity_path.read_text())["runtime"]["uid"], "vm")

    def test_stable_guest_control_checks_terminal_not_pixels(self):
        response = MagicMock()
        response.status = 200
        with patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(adapter.request, "urlopen") as urlopen, \
             patch.object(adapter.time, "sleep") as sleep:
            urlopen.return_value.__enter__.return_value = response
            self.episode._stable_guest_control()
        self.assertEqual(urlopen.call_count, 4)
        self.assertTrue(all(call.args[0].endswith("/terminal") for call in urlopen.call_args_list))
        self.assertEqual(sleep.call_count, 3)

    def test_overlay_rejects_unprotected_base_and_persistent_boot_path_before_exec(self):
        pod = {"spec": {"containers": [{"name": "qemu", "volumeMounts": [
            {"name": "image", "mountPath": "/System.qcow2", "subPath": "System.qcow2", "readOnly": True}]}],
            "volumes": [{"name": "image", "persistentVolumeClaim": {"claimName": "osworld-v1-hot", "readOnly": True}}]}}
        pod["spec"]["containers"][0]["volumeMounts"][0]["readOnly"] = False
        with patch.object(self.episode, "_pod_snapshot", return_value={"uid": "vm"}), \
             patch.object(self.episode, "get", return_value=pod), \
             patch.object(self.episode, "kubectl") as kubectl:
            with self.assertRaisesRegex(ValueError, "read-only"):
                self.episode._overlay()
            kubectl.assert_not_called()
        pod["spec"]["containers"][0]["volumeMounts"][0]["readOnly"] = True
        pod["spec"]["volumes"][0]["persistentVolumeClaim"]["readOnly"] = False
        with patch.object(self.episode, "_pod_snapshot", return_value={"uid": "vm"}), \
             patch.object(self.episode, "get", return_value=pod), \
             patch.object(self.episode, "kubectl") as kubectl:
            with self.assertRaisesRegex(ValueError, "PVC source is not read-only"):
                self.episode._overlay()
            kubectl.assert_not_called()
        pod["spec"]["volumes"][0]["persistentVolumeClaim"]["readOnly"] = True
        pod["spec"]["containers"][0]["volumeMounts"].append({"name": "other", "mountPath": "/boot.qcow2"})
        with patch.object(self.episode, "_pod_snapshot", return_value={"uid": "vm"}), \
             patch.object(self.episode, "get", return_value=pod), \
             patch.object(self.episode, "kubectl") as kubectl:
            with self.assertRaisesRegex(ValueError, "covered by a Pod volume mount"):
                self.episode._overlay()
            kubectl.assert_not_called()

    def test_live_v1_boot_accepts_verified_disposable_backing_file_overlay(self):
        # ROOT CAUSE:
        # The pinned runtime creates /boot.qcow2 with /System.qcow2 as its
        # backing file. The old assertion required QEMU -snapshot and rejected
        # the real fresh overlay before the episode could install AUV.
        pod = {
            "metadata": {"uid": "runtime-uid", "labels": self.episode._labels("qemu")},
            "spec": {"nodeName": "liet-gpu-1", "containers": [{
                "name": "qemu", "image": adapter.RUNTIME_IMAGE,
                "volumeMounts": [{"name": "image", "mountPath": "/System.qcow2",
                                  "subPath": "System.qcow2", "readOnly": True}],
            }], "volumes": [{"name": "image", "persistentVolumeClaim": {
                "claimName": "osworld-v1-hot", "readOnly": True}}]},
            "status": {"containerStatuses": [{"name": "qemu", "ready": True,
                "restartCount": 0, "containerID": "containerd://live-container",
                "imageID": "docker.io/" + adapter.RUNTIME_IMAGE}]},
        }
        overrides = {}

        def observed_command(*args, **_kwargs):
            if "qemu-img" in args and "qemu-img" in overrides:
                return overrides["qemu-img"]
            if "/bin/sh" in args and "/bin/sh" in overrides:
                return overrides["/bin/sh"]
            if "ps" in args:
                return "42 " + LIVE_V1_QEMU_ARGV + "\n"
            if "qemu-img" in args:
                return json.dumps({"filename": "/boot.qcow2", "format": "qcow2",
                                   "backing-filename": "/System.qcow2",
                                   "full-backing-filename": "/System.qcow2",
                                   "backing-filename-format": "qcow2"})
            if "findmnt" in args:
                return json.dumps({"filesystems": [{"target": "/", "fstype": "overlay", "source": "overlay"}]})
            if "sha256sum" in args:
                return "b" * 64 + "  /System.qcow2\n"
            if "/bin/sh" in args:
                return "/boot.qcow2\n/System.qcow2\n"
            self.fail(f"unexpected cluster command: {args}")

        with patch.object(self.episode, "get", return_value=pod), \
             patch.object(self.episode, "kubectl", side_effect=observed_command):
            evidence = self.episode._overlay()
        self.assertEqual(evidence["base_qcow_sha256"], "b" * 64)
        self.assertEqual(evidence["backing_file"], "/System.qcow2")
        self.assertEqual(evidence["boot_file"], "/boot.qcow2")
        self.assertEqual(evidence["runtime"]["uid"], "runtime-uid")

        overrides["qemu-img"] = json.dumps({"filename": "/boot.qcow2", "format": "qcow2",
            "backing-filename": "/different.qcow2", "full-backing-filename": "/different.qcow2",
            "backing-filename-format": "qcow2"})
        with patch.object(self.episode, "get", return_value=pod), \
             patch.object(self.episode, "kubectl", side_effect=observed_command):
            with self.assertRaisesRegex(ValueError, "backing file"):
                self.episode._overlay()
        del overrides["qemu-img"]
        overrides["/bin/sh"] = "/System.qcow2\n"
        with patch.object(self.episode, "get", return_value=pod), \
             patch.object(self.episode, "kubectl", side_effect=observed_command):
            with self.assertRaisesRegex(ValueError, "has not opened"):
                self.episode._overlay()

    def test_identity_fails_if_container_restarted_under_same_pod_uid(self):
        adapter.write_json(self.episode.identity_path, {"runtime": {"uid": "same", "container_id": "old", "restart_count": 0},
                                                        "proxy": {"uid": "proxy"}, "service_uid": "svc"})
        with patch.object(self.episode, "_pod_snapshot", side_effect=[
            {"uid": "same", "container_id": "new", "restart_count": 1}, {"uid": "proxy"}]):
            with self.assertRaisesRegex(ValueError, "identity changed"):
                self.episode.assert_identity()

    def test_forward_is_in_runner_process_group_and_always_closes(self):
        process = MagicMock()
        process.poll.return_value = None
        process.pid = 1234
        def spawn(_argv, **kwargs):
            kwargs["stdout"].write(f"Forwarding from 127.0.0.1:{self.episode.config['setup_local_port']} -> 5000\n".encode())
            kwargs["stdout"].write(f"Forwarding from 127.0.0.1:{self.episode.config['auv_local_port']} -> 8080\n".encode())
            kwargs["stdout"].flush()
            return process
        with patch.object(self.episode, "assert_identity"), \
             patch.object(adapter.subprocess, "Popen", side_effect=spawn) as popen, \
             patch.object(adapter.socket, "create_connection") as connect:
            connect.return_value.__enter__.return_value = None
            with self.assertRaisesRegex(RuntimeError, "phase failed"):
                with self.episode.forward(setup=True, auv=True):
                    raise RuntimeError("phase failed")
        self.assertFalse(popen.call_args.kwargs["start_new_session"])
        self.assertNotEqual(popen.call_args.kwargs["stderr"], adapter.subprocess.PIPE)
        self.assertTrue((self.directory / "port-forward-setup-auv.stderr").exists())
        self.assertEqual([call.args[0][1] for call in connect.call_args_list],
                         [self.episode.config["setup_local_port"], self.episode.config["auv_local_port"]])
        process.terminate.assert_called_once()
        process.wait.assert_called_once()

    def test_forward_refuses_occupied_port_before_spawn(self):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as occupied:
            occupied.bind(("127.0.0.1", 0))
            self.episode.config["setup_local_port"] = occupied.getsockname()[1]
            with patch.object(self.episode, "assert_identity"), patch.object(adapter.subprocess, "Popen") as popen:
                with self.assertRaisesRegex(ValueError, "occupied local port"):
                    with self.episode.forward(setup=True):
                        self.fail("occupied listener must not be accepted")
            popen.assert_not_called()

    def test_forward_failure_keeps_stderr_without_blocking_pipe(self):
        process = MagicMock()
        process.poll.return_value = 1

        def spawn(_argv, **kwargs):
            kwargs["stderr"].write(b"forward-failed")
            kwargs["stderr"].flush()
            return process

        with patch.object(self.episode, "assert_identity"), patch.object(adapter.subprocess, "Popen", side_effect=spawn):
            with self.assertRaisesRegex(RuntimeError, "forward-failed"):
                with self.episode.forward(setup=True):
                    self.fail("failed forward must not yield")
        self.assertIn(b"forward-failed", (self.directory / "port-forward-setup-control.stderr").read_bytes())

    def test_action_uses_only_auv_capture_and_writes_auditable_evidence(self):
        image = self.directory / "produced.png"
        image.write_bytes(b"auv-captured-pixels")
        (self.directory / "paired-profiles.json").write_text("paired-secret")
        adapter.write_json(self.directory / "paired-device.json", {"device_id": "canonical-device"})
        sidecar = self.directory / "action_evidence.json"
        capture = {"run_id": "run-123", "artifacts": [{"purpose": "auv.driver.display_capture", "file_path": str(image)}]}
        with patch.dict(os.environ, {"AUV_OSWORLD_ACTION_EVIDENCE": str(sidecar)}), \
             patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(adapter, "_run", return_value=json.dumps(capture)) as run:
            self.episode.action()
        argv = run.call_args.args[0]
        self.assertEqual(argv[1:6], ["--device", "canonical-device", "invoke", "display.capture", "--json"])
        self.assertFalse(any("input." in value for value in argv))
        evidence = json.loads(sidecar.read_text())
        self.assertEqual(evidence["run_ids"], ["run-123"])
        self.assertEqual(evidence["final_artifact"]["sha256"], hashlib.sha256(b"auv-captured-pixels").hexdigest())

    def test_reset_refuses_replaced_uid_before_deletion(self):
        adapter.write_json(self.episode.owned_path, [{"kind": "pod", "name": "chrome-vm", "uid": "old"}])
        secret = self.directory / "paired-profiles.json"
        secret.write_text("task-owned bearer")
        with patch.object(self.episode, "api_proxy", return_value=nullcontext("http://127.0.0.1:12345")), \
             patch.object(self.episode, "get", return_value={"metadata": {"uid": "new", "labels": self.episode._labels("qemu")}}), \
             patch.object(self.episode, "kubectl") as kubectl:
            with self.assertRaisesRegex(ValueError, "replaced"):
                self.episode.reset()
        kubectl.assert_not_called()
        self.assertFalse(secret.exists())

    def test_delete_uses_kubernetes_uid_precondition_in_atomic_delete_body(self):
        item = {"kind": "pod", "name": "chrome-vm", "uid": "exact-uid"}
        response = MagicMock()
        response.status = 200
        with patch.object(adapter.request, "urlopen") as urlopen:
            urlopen.return_value.__enter__.return_value = response
            self.episode.request_deletion("http://127.0.0.1:12345", item)
        outgoing = urlopen.call_args.args[0]
        self.assertEqual(outgoing.get_method(), "DELETE")
        self.assertEqual(outgoing.full_url, "http://127.0.0.1:12345/api/v1/namespaces/bench/pods/chrome-vm")
        self.assertEqual(json.loads(outgoing.data), {
            "apiVersion": "v1", "kind": "DeleteOptions", "preconditions": {"uid": "exact-uid"},
        })

    def test_delete_waits_past_pod_grace_period_for_accepted_uid(self):
        # ROOT CAUSE:
        # If an accepted Pod DELETE disappears just after its 30-second grace
        # period, the old 30-second observation budget falsely failed reset.
        # The fix keeps the UID-preconditioned request and waits separately
        # long enough to observe the same object actually disappear.
        item = {"kind": "pod", "name": "chrome-vm", "uid": "exact-uid"}
        now = [0.0]

        def get_after_grace(_kind, _name):
            if now[0] >= 40:
                raise RuntimeError("NotFound")
            return {"metadata": {"uid": "exact-uid"}}

        with patch.object(self.episode, "get", side_effect=get_after_grace), \
             patch.object(adapter.time, "monotonic", side_effect=lambda: now[0]), \
             patch.object(adapter.time, "sleep", side_effect=lambda seconds: now.__setitem__(0, now[0] + seconds)):
            self.episode.wait_deleted([item])
        self.assertGreaterEqual(now[0], 40)

    def test_shared_delete_budget_reports_pending_uid_without_false_success(self):
        items = [
            {"kind": "pod", "name": "chrome-vm", "uid": "vm-uid"},
            {"kind": "service", "name": "chrome-svc", "uid": "svc-uid"},
        ]
        now = [0.0]

        def get_one_stuck(kind, name):
            if kind == "service":
                raise RuntimeError("NotFound")
            return {"metadata": {"uid": "vm-uid"}}

        with patch.object(self.episode, "get", side_effect=get_one_stuck), \
             patch.object(adapter.time, "monotonic", side_effect=lambda: now[0]), \
             patch.object(adapter.time, "sleep", side_effect=lambda seconds: now.__setitem__(0, now[0] + seconds)):
            with self.assertRaisesRegex(TimeoutError, "pod/chrome-vm uid=vm-uid"):
                self.episode.wait_deleted(items, timeout_seconds=1)

    def test_reset_requests_all_uid_deletes_before_shared_wait(self):
        owned = [
            {"kind": "pod", "name": "chrome-vm", "uid": "vm-uid"},
            {"kind": "service", "name": "chrome-svc", "uid": "svc-uid"},
            {"kind": "pod", "name": "chrome-proxy", "uid": "proxy-uid"},
        ]
        adapter.write_json(self.episode.owned_path, owned)
        events = []

        def observed(kind, name):
            uid = next(item["uid"] for item in owned if item["kind"] == kind and item["name"] == name)
            role = "qemu" if name == "chrome-vm" else "proxy" if name == "chrome-proxy" else None
            return {"metadata": {"uid": uid, "labels": self.episode._labels(role)}}

        with patch.object(self.episode, "api_proxy", return_value=nullcontext("http://127.0.0.1:12345")), \
             patch.object(self.episode, "get", side_effect=observed), \
             patch.object(self.episode, "request_deletion", side_effect=lambda _origin, item: events.append(("delete", item["uid"]))), \
             patch.object(self.episode, "wait_deleted", side_effect=lambda items: events.append(("wait", [item["uid"] for item in items]))), \
             patch.object(self.episode, "_retained_pvc", return_value={"name": "osworld-v1-hot"}):
            self.episode.reset()
        self.assertEqual(events, [
            ("delete", "proxy-uid"), ("delete", "svc-uid"), ("delete", "vm-uid"),
            ("wait", ["vm-uid", "svc-uid", "proxy-uid"]),
        ])

    def test_api_proxy_stays_in_runner_group_and_is_closed(self):
        process = MagicMock()
        process.poll.return_value = None
        process.stdout.readline.return_value = "Starting to serve on 127.0.0.1:45678\n"
        with patch.object(adapter.subprocess, "Popen", return_value=process) as popen:
            with self.episode.api_proxy() as origin:
                self.assertEqual(origin, "http://127.0.0.1:45678")
        self.assertFalse(popen.call_args.kwargs["start_new_session"])
        self.assertNotEqual(popen.call_args.kwargs["stderr"], adapter.subprocess.PIPE)
        self.assertTrue((self.directory / "kube-api-proxy.stderr").exists())
        process.terminate.assert_called_once()

    def test_reset_requires_bound_pvc_with_original_pv_identity(self):
        adapter.write_json(self.episode.owned_path, [])
        adapter.write_json(self.episode.identity_path, {"retained_pvc": {"name": "osworld-v1-hot", "uid": "pvc-1", "pv_uid": "pv-1"}})
        with patch.object(self.episode, "_retained_pvc", return_value={"name": "osworld-v1-hot", "uid": "pvc-2", "pv_uid": "pv-1"}):
            with self.assertRaisesRegex(ValueError, "identity changed"):
                self.episode.reset()

    def test_reset_rejects_journal_entry_outside_task_owned_names(self):
        adapter.write_json(self.episode.owned_path, [{"kind": "pod", "name": "other-workload", "uid": "uid"}])
        with patch.object(self.episode, "kubectl") as kubectl:
            with self.assertRaisesRegex(ValueError, "unapproved"):
                self.episode.reset()
        kubectl.assert_not_called()


if __name__ == "__main__":
    unittest.main()
