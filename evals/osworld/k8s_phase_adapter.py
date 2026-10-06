"""Fail-closed Kubernetes phase adapter for one OSWorld V1 Chrome control.

This is scheduler plumbing, not an agent. The action is a paired-AUV capture
only, so a score of zero is a negative control, never an AUV solving attempt.
No configuration field can select a GUI action executable or Python source.
"""

from __future__ import annotations

from contextlib import contextmanager, nullcontext
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from urllib import request


V1_REVISION = "b138d348256078fa634fc3b73567a7337c793e6b"
CHROME_TASK = "2ad9387a-65d8-4e33-ad5b-7580065a27ca"
CHROME_SHA256 = "4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c"
RUNTIME_IMAGE = "happysixd/osworld-docker@sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9"
AUV_SOURCE = "25e2320570a72d3b9580451ea2917a9e03fa6b95"
GUEST_AUV_SHA256 = "2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427"
HOST_AUV_SHA256 = "cf9485c4a2ec0fbf14c0fa6f874ef77decba00f8c61ae704ec67b77915a3c08a"
PHASES = ("boot", "install", "setup", "action", "evaluate", "reset")
TIMEOUTS = {"boot": 900, "install": 300, "setup": 180, "action": 600, "evaluate": 180, "reset": 180}
CONFIG_FIELDS = (
    "batch_id", "episode_id", "namespace", "kubeconfig", "context", "node", "runtime_pod",
    "runtime_service", "proxy_pod", "proxy_image", "base_pvc", "base_qcow_sha256",
    "guest_auv_binary", "host_auv_binary", "upstream_checkout", "setup_local_port", "auv_local_port",
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path: Path, value: dict) -> None:
    with tempfile.NamedTemporaryFile("w", dir=path.parent, prefix=f".{path.name}-", delete=False) as stream:
        temp = Path(stream.name)
        try:
            json.dump(value, stream, sort_keys=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        except BaseException:
            temp.unlink(missing_ok=True)
            raise
    os.replace(temp, path)


def load_config(path: Path) -> dict:
    config = json.loads(path.read_text())
    if not isinstance(config, dict) or set(config) != set(CONFIG_FIELDS):
        raise ValueError(f"config must contain exactly: {', '.join(CONFIG_FIELDS)}")
    for name in CONFIG_FIELDS:
        if name not in ("setup_local_port", "auv_local_port") and (not isinstance(config[name], str) or not config[name].strip()):
            raise ValueError(f"{name} must be a nonempty string")
    for name in ("batch_id", "episode_id", "namespace", "runtime_pod", "runtime_service", "proxy_pod", "base_pvc"):
        if not re.fullmatch(r"[a-z0-9]([-a-z0-9]*[a-z0-9])?", config[name]) or len(config[name]) > 63:
            raise ValueError(f"{name} must be a Kubernetes DNS label")
    if len({config["runtime_pod"], config["runtime_service"], config["proxy_pod"]}) != 3:
        raise ValueError("runtime Pod, Service, and proxy Pod names must differ")
    if not re.fullmatch(r"[^\s]+@sha256:[0-9a-f]{64}", config["proxy_image"]):
        raise ValueError("proxy_image must be pinned by digest and contain socat")
    if not re.fullmatch(r"[0-9a-f]{64}", config["base_qcow_sha256"]):
        raise ValueError("base_qcow_sha256 needs a measured digest; V1 archive was not hash-verified")
    ports = (config["setup_local_port"], config["auv_local_port"])
    if any(isinstance(port, bool) or not isinstance(port, int) or not 1024 <= port <= 65535 for port in ports) or ports[0] == ports[1]:
        raise ValueError("distinct unprivileged integer local ports are required")
    for name in ("kubeconfig", "guest_auv_binary", "host_auv_binary", "upstream_checkout"):
        value = Path(config[name])
        if not value.is_absolute() or not value.exists():
            raise ValueError(f"{name} must be an existing absolute path")
    if sha256(Path(config["guest_auv_binary"])) != GUEST_AUV_SHA256:
        raise ValueError("guest AUV binary differs from the pinned validated Ubuntu 22.04 build")
    if sha256(Path(config["host_auv_binary"])) != HOST_AUV_SHA256:
        raise ValueError("host AUV binary differs from the pinned paired-client build")
    revision = subprocess.run(["git", "-C", config["upstream_checkout"], "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    if revision != V1_REVISION:
        raise ValueError("upstream V1 checkout revision differs from the pinned evaluator")
    if subprocess.run(["git", "-C", config["upstream_checkout"], "status", "--porcelain"], capture_output=True, text=True, check=True).stdout.strip():
        raise ValueError("upstream V1 checkout must be clean")
    task = Path(config["upstream_checkout"]) / "evaluation_examples/examples/chrome" / f"{CHROME_TASK}.json"
    if sha256(task) != CHROME_SHA256:
        raise ValueError("Chrome task JSON differs from the pinned task")
    return config


def manifest(config_path: Path) -> dict:
    config_path = config_path.resolve(strict=True)
    config = load_config(config_path)
    script = Path(__file__).resolve()
    identity = {
        "benchmark": "OSWorld-V1", "benchmark_revision": V1_REVISION, "task_id": CHROME_TASK,
        "task_sha256": CHROME_SHA256, "topology": "paired-remote-capture-only-negative-control",
        "runtime_image": RUNTIME_IMAGE, "qcow2": f"sha256:{config['base_qcow_sha256']}",
        "auv_source": AUV_SOURCE, "auv_binary_sha256": GUEST_AUV_SHA256,
        "auv_target": "paired Device ID acquired at install", "runner_identity": "k8s_phase_adapter.py capture-only",
    }
    phases = {name: {"argv": [sys.executable, str(script), "phase", name, "--config", str(config_path)],
                     "timeout_seconds": TIMEOUTS[name]} for name in PHASES}
    return {"trust": "operator-audited", "batch_id": config["batch_id"],
            "episodes": [{"episode_id": config["episode_id"], "identity": identity, "phases": phases}]}


def _run(argv: list[str], *, env: dict | None = None, input_text: str | None = None) -> str:
    result = subprocess.run(argv, input=input_text, capture_output=True, text=True, env=env, check=False)
    if result.returncode:
        raise RuntimeError(f"{argv[0]} exited {result.returncode}: {result.stderr[-1000:]}")
    return result.stdout


class Episode:
    def __init__(self, config: dict, directory: Path):
        self.config = config
        self.directory = directory
        self.identity_path = directory / "k8s_identity.json"
        self.owned_path = directory / "k8s_owned.json"

    def kubectl(self, *args: str, input_text: str | None = None) -> str:
        return _run(["kubectl", "--kubeconfig", self.config["kubeconfig"], "--context", self.config["context"],
                     "-n", self.config["namespace"], *args], input_text=input_text)

    def get(self, kind: str, name: str) -> dict:
        return json.loads(self.kubectl("get", kind, name, "-o", "json"))

    def _labels(self, role: str | None = None) -> dict:
        labels = {"app.kubernetes.io/name": "auv-osworld-control", "auv.moeru.ai/batch": self.config["batch_id"],
                  "auv.moeru.ai/episode": self.config["episode_id"]}
        if role is not None:
            labels["auv.moeru.ai/role"] = role
        return labels

    def _create(self, resource: dict) -> None:
        kind = resource["kind"].lower()
        name = resource["metadata"]["name"]
        try:
            self.get(kind, name)
        except RuntimeError as error:
            if "NotFound" not in str(error) and "not found" not in str(error):
                raise
        else:
            raise ValueError(f"refusing pre-existing {kind}/{name}")
        self.kubectl("create", "-f", "-", input_text=json.dumps(resource))
        observed = self.get(kind, name)
        if observed["metadata"].get("labels") != resource["metadata"]["labels"]:
            raise ValueError(f"created {kind}/{name} has unexpected ownership labels")
        owned = json.loads(self.owned_path.read_text()) if self.owned_path.exists() else []
        owned.append({"kind": kind, "name": name, "uid": observed["metadata"]["uid"]})
        write_json(self.owned_path, owned)

    def _pod_snapshot(self, name: str, container: str, expected_image: str) -> dict:
        pod = self.get("pod", name)
        if pod["metadata"].get("labels") != self._labels(container) or pod["spec"].get("nodeName") != self.config["node"]:
            raise ValueError(f"pod/{name} owner or node mismatch")
        spec = next(item for item in pod["spec"]["containers"] if item["name"] == container)
        status = next(item for item in pod["status"]["containerStatuses"] if item["name"] == container)
        if spec["image"] != expected_image or not status.get("ready") or status.get("restartCount") != 0:
            raise ValueError(f"pod/{name} image, readiness, or restart mismatch")
        digest = expected_image.split("@", 1)[1]
        if digest not in status.get("imageID", ""):
            raise ValueError(f"pod/{name} actual image digest mismatch")
        return {"uid": pod["metadata"]["uid"], "container_id": status["containerID"],
                "restart_count": status["restartCount"], "image_id": status["imageID"]}

    def _overlay(self) -> dict:
        runtime = self.get("pod", self.config["runtime_pod"])
        container = next(item for item in runtime["spec"]["containers"] if item["name"] == "qemu")
        image_mount = next(item for item in container["volumeMounts"] if item["name"] == "image")
        image_volume = next(item for item in runtime["spec"]["volumes"] if item["name"] == "image")
        if image_mount != {"name": "image", "mountPath": "/System.qcow2", "subPath": "System.qcow2", "readOnly": True}:
            raise ValueError("base qcow2 is not mounted read-only at the audited path")
        if image_volume["persistentVolumeClaim"] != {"claimName": self.config["base_pvc"]}:
            raise ValueError("base qcow2 PVC mismatch")
        processes = self.kubectl("exec", self.config["runtime_pod"], "-c", "qemu", "--", "ps", "-eo", "args")
        qemu = [line for line in processes.splitlines() if "qemu-system" in line and "-enable-kvm" in line]
        if len(qemu) != 1 or "-snapshot" not in qemu[0] or "/System.qcow2" not in qemu[0]:
            raise ValueError("live QEMU command does not prove KVM and a disposable -snapshot overlay")
        guest_hash = self.kubectl("exec", self.config["runtime_pod"], "-c", "qemu", "--", "sha256sum", "/System.qcow2").split()[0]
        if guest_hash != self.config["base_qcow_sha256"]:
            raise ValueError("mounted V1 base qcow2 SHA256 mismatch")
        return {"qemu_argv": qemu[0], "base_qcow_sha256": guest_hash, "overlay": "QEMU -snapshot on read-only base"}

    def assert_identity(self) -> dict:
        identity = json.loads(self.identity_path.read_text())
        if self._pod_snapshot(self.config["runtime_pod"], "qemu", RUNTIME_IMAGE) != identity["runtime"]:
            raise ValueError("runtime Pod UID/container identity changed")
        if self._pod_snapshot(self.config["proxy_pod"], "proxy", self.config["proxy_image"]) != identity["proxy"]:
            raise ValueError("proxy Pod UID/container identity changed")
        service = self.get("service", self.config["runtime_service"])
        if service["metadata"]["uid"] != identity["service_uid"] or service["spec"].get("selector") != self._labels("qemu"):
            raise ValueError("runtime Service identity or selector changed")
        # NOTICE: Kubernetes may restart a container without changing Pod UID.
        # The container ID and restart count above must therefore be stable.
        return identity

    @contextmanager
    def forward(self, *, setup: bool = False, auv: bool = False):
        """A phase owns its forward; no detached host listener survives it."""
        self.assert_identity()
        ports = []
        if setup:
            ports.append(f"{self.config['setup_local_port']}:5000")
        if auv:
            ports.append(f"{self.config['auv_local_port']}:8080")
        if not ports:
            raise ValueError("forward needs an endpoint")
        local_ports = [int(mapping.split(":", 1)[0]) for mapping in ports]
        for port in local_ports:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
                try:
                    probe.bind(("127.0.0.1", port))
                except OSError as error:
                    raise ValueError(f"refusing occupied local port {port}") from error
        argv = ["kubectl", "--kubeconfig", self.config["kubeconfig"], "--context", self.config["context"],
                "-n", self.config["namespace"],
                "port-forward", f"pod/{self.config['proxy_pod']}", *ports, "--address", "127.0.0.1"]
        # Stay in the runner's process group. Its hard timeout must kill this
        # network path even if SIGTERM interrupts Python before finally runs.
        label = f"{'setup-' if setup else ''}{'auv' if auv else 'control'}"
        log_path = self.directory / f"port-forward-{label}.stderr"
        output_path = self.directory / f"port-forward-{label}.stdout"
        existing_bytes = output_path.stat().st_size if output_path.exists() else 0
        with log_path.open("ab") as log, output_path.open("ab") as output:
            process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=output,
                                       stderr=log, start_new_session=False)
            try:
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError(f"port-forward exited; stderr tail: {log_path.read_bytes()[-500:].decode(errors='replace')}")
                    ready = output_path.read_bytes()[existing_bytes:]
                    if all(f"Forwarding from 127.0.0.1:{port} -> ".encode() in ready for port in local_ports):
                        try:
                            for port in local_ports:
                                with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                                    pass
                        except OSError:
                            time.sleep(0.1)
                            continue
                        if process.poll() is None:
                            break
                    time.sleep(0.1)
                else:
                    raise TimeoutError("port-forward did not prove every requested listener")
                self.assert_identity()
                yield
                self.assert_identity()
            finally:
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=2)

    @contextmanager
    def api_proxy(self):
        """Use kubectl auth for an atomic UID-preconditioned Kubernetes DELETE."""
        argv = ["kubectl", "--kubeconfig", self.config["kubeconfig"], "--context", self.config["context"],
                "proxy", "--address=127.0.0.1", "--port=0"]
        log_path = self.directory / "kube-api-proxy.stderr"
        with log_path.open("ab") as log:
            process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                       stderr=log, text=True, start_new_session=False)
            try:
                line = process.stdout.readline().strip()
                match = re.fullmatch(r"Starting to serve on 127\.0\.0\.1:([0-9]+)", line)
                if not match or process.poll() is not None:
                    raise RuntimeError(f"kubectl proxy did not start on loopback; stderr tail: {log_path.read_bytes()[-500:].decode(errors='replace')}")
                yield f"http://127.0.0.1:{match.group(1)}"
            finally:
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=2)

    def delete_owned(self, origin: str, item: dict) -> None:
        """Kubernetes checks UID under its DELETE lock, not in a prior GET."""
        plural = "pods" if item["kind"] == "pod" else "services" if item["kind"] == "service" else None
        if plural is None:
            raise ValueError("unapproved resource kind for deletion")
        url = f"{origin}/api/v1/namespaces/{self.config['namespace']}/{plural}/{item['name']}"
        body = {"apiVersion": "v1", "kind": "DeleteOptions", "preconditions": {"uid": item["uid"]}}
        deletion = request.Request(url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}, method="DELETE")
        with request.urlopen(deletion, timeout=30) as response:
            if response.status not in (200, 202):
                raise RuntimeError(f"Kubernetes UID-preconditioned DELETE returned HTTP {response.status}")
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            try:
                remaining = self.get(item["kind"], item["name"])
            except RuntimeError as error:
                if "NotFound" in str(error) or "not found" in str(error):
                    return
                raise
            if remaining["metadata"].get("uid") != item["uid"]:
                raise ValueError(f"{item['kind']}/{item['name']} was replaced after deletion")
            time.sleep(0.25)
        raise TimeoutError(f"UID-preconditioned deletion did not remove {item['kind']}/{item['name']}")

    def _post(self, route: str, value: dict) -> dict:
        url = f"http://127.0.0.1:{self.config['setup_local_port']}{route}"
        body = json.dumps(value).encode()
        with request.urlopen(request.Request(url, data=body, headers={"Content-Type": "application/json"}), timeout=30) as response:
            return json.load(response)

    def _stable_guest_control(self) -> None:
        """Check a non-GUI endpoint across the boot/reboot window."""
        deadline = time.monotonic() + 120
        consecutive = 0
        with self.forward(setup=True):
            while consecutive < 4:
                if time.monotonic() >= deadline:
                    raise TimeoutError("guest control API did not stay available across 15 seconds")
                try:
                    with request.urlopen(f"http://127.0.0.1:{self.config['setup_local_port']}/terminal", timeout=15) as response:
                        if response.status != 200:
                            raise ValueError("guest /terminal did not return HTTP 200")
                except (OSError, ValueError):
                    consecutive = 0
                else:
                    consecutive += 1
                if consecutive < 4:
                    time.sleep(5)

    def guest_control(self, command: list[str]) -> dict:
        """Only installation/control-plane calls are permitted, never AUV GUI."""
        allowed = (
            ["chmod", "0700", "/home/user/auv"],
            ["sha256sum", "/home/user/auv"],
            ["/home/user/auv", "--version"],
            ["env", "AUV_ENDPOINT=unix:///home/user/auv.sock", "/home/user/auv", "devices", "pair", "create-token"],
        )
        if command not in allowed:
            raise ValueError("unreviewed guest command or AUV GUI invoke is forbidden")
        return self._post("/setup/execute", {"command": command, "shell": False})

    def boot(self) -> None:
        c = self.config
        labels = self._labels()
        pod = {"apiVersion": "v1", "kind": "Pod", "metadata": {"name": c["runtime_pod"], "labels": self._labels("qemu")},
               "spec": {"nodeSelector": {"kubernetes.io/hostname": c["node"]}, "terminationGracePeriodSeconds": 30,
                        "containers": [{"name": "qemu", "image": RUNTIME_IMAGE, "imagePullPolicy": "IfNotPresent",
                                        "securityContext": {"privileged": True},
                                        "env": [{"name": "DISK_SIZE", "value": "32G"}, {"name": "RAM_SIZE", "value": "8G"},
                                                {"name": "CPU_CORES", "value": "4"}],
                                        "resources": {"requests": {"cpu": "4", "memory": "8Gi"}, "limits": {"cpu": "8", "memory": "12Gi"}},
                                        "startupProbe": {"tcpSocket": {"port": 5000}, "periodSeconds": 5, "failureThreshold": 120},
                                        "readinessProbe": {"tcpSocket": {"port": 5000}, "periodSeconds": 5,
                                                           "failureThreshold": 3},
                                        "volumeMounts": [{"name": "image", "mountPath": "/System.qcow2", "subPath": "System.qcow2", "readOnly": True},
                                                         {"name": "kvm", "mountPath": "/dev/kvm"}]}],
                        "volumes": [{"name": "image", "persistentVolumeClaim": {"claimName": c["base_pvc"]}},
                                    {"name": "kvm", "hostPath": {"path": "/dev/kvm", "type": "CharDevice"}}]}}
        service = {"apiVersion": "v1", "kind": "Service", "metadata": {"name": c["runtime_service"], "labels": labels},
                   "spec": {"selector": self._labels("qemu"), "ports": [{"name": "setup", "port": 5000, "targetPort": 5000},
                                                          {"name": "auv", "port": 8080, "targetPort": 8080}]}}
        proxy_command = (f"socat TCP-LISTEN:5000,fork,reuseaddr TCP:{c['runtime_service']}:5000 & "
                         f"socat TCP-LISTEN:8080,fork,reuseaddr TCP:{c['runtime_service']}:8080 & wait")
        proxy = {"apiVersion": "v1", "kind": "Pod", "metadata": {"name": c["proxy_pod"], "labels": self._labels("proxy")},
                 "spec": {"restartPolicy": "Never", "nodeSelector": {"kubernetes.io/hostname": c["node"]},
                          "containers": [{"name": "proxy", "image": c["proxy_image"], "command": ["/bin/sh", "-ec", proxy_command],
                                          "readinessProbe": {"tcpSocket": {"port": 5000}, "periodSeconds": 2, "failureThreshold": 30}}]}}
        retained = self._retained_pvc()
        for resource in (pod, service, proxy):
            self._create(resource)
        self.kubectl("wait", "--for=condition=Ready", f"pod/{c['runtime_pod']}", "--timeout=15m")
        self.kubectl("wait", "--for=condition=Ready", f"pod/{c['proxy_pod']}", "--timeout=2m")
        runtime = self._pod_snapshot(c["runtime_pod"], "qemu", RUNTIME_IMAGE)
        proxy_id = self._pod_snapshot(c["proxy_pod"], "proxy", c["proxy_image"])
        service_uid = self.get("service", c["runtime_service"])["metadata"]["uid"]
        overlay = self._overlay()
        write_json(self.identity_path, {"runtime": runtime, "proxy": proxy_id, "service_uid": service_uid,
                                        "overlay": overlay, "retained_pvc": retained})
        self._stable_guest_control()
        self.assert_identity()
        print(json.dumps({"phase": "boot", "runtime_uid": runtime["uid"], "overlay": overlay}))

    def install(self) -> None:
        self.assert_identity()
        c = self.config
        profile_path = self.directory / "paired-profiles.json"
        if profile_path.exists():
            raise FileExistsError("refusing to reuse existing paired profile")
        with self.forward(setup=True, auv=True):
            _run(["curl", "--fail-with-body", "--silent", "--show-error", "-F", "file_path=/home/user/auv",
                  "-F", f"file_data=@{c['guest_auv_binary']}", f"http://127.0.0.1:{c['setup_local_port']}/setup/upload"])
            self.guest_control(["chmod", "0700", "/home/user/auv"])
            measured = self.guest_control(["sha256sum", "/home/user/auv"]).get("output", "").split()[0]
            if measured != GUEST_AUV_SHA256:
                raise ValueError("guest-installed AUV bytes differ")
            version = self.guest_control(["/home/user/auv", "--version"])
            daemon = ["env", "DISPLAY=:0", "XDG_SESSION_TYPE=x11", "/home/user/auv", "serve",
                      "--listen", "unix:///home/user/auv.sock", "--listen", "http://0.0.0.0:8080",
                      "--pairing-store", "/home/user/.local/share/auv-osworld/pairings.json",
                      "--store-root", "/home/user/.local/share/auv-osworld", "--no-register"]
            self._post("/setup/launch", {"command": daemon, "shell": False})
            token = self.guest_control(["env", "AUV_ENDPOINT=unix:///home/user/auv.sock", "/home/user/auv", "devices", "pair", "create-token"]).get("output", "").strip()
            if not token or "\n" in token:
                raise ValueError("owner socket did not return one pairing token")
            env = {**os.environ, "AUV_CONFIG_PROFILES_FILE": str(profile_path),
                   "AUV_DISCOVERY_FILE": str(self.directory / "no-local-discovery.json")}
            result = _run([c["host_auv_binary"], "devices", "pair", "--endpoint",
                           f"http://127.0.0.1:{c['auv_local_port']}", "connect", "--token-stdin",
                           "--label", c["episode_id"], "--profile", c["episode_id"], "--json"], env=env, input_text=token)
            paired = json.loads(result)
            if not isinstance(paired.get("device_id"), str) or not paired["device_id"]:
                raise ValueError("pair connect did not return a Device ID")
            write_json(self.directory / "paired-device.json", {"device_id": paired["device_id"],
                                                              "guest_auv_sha256": measured, "version": version})
        self.assert_identity()
        print(json.dumps({"phase": "install", "device_id": paired["device_id"], "guest_auv_sha256": measured}))

    def evaluator(self, phase: str) -> None:
        self.assert_identity()
        if not (self.directory / "paired-device.json").exists():
            raise ValueError("install evidence missing")
        with self.forward(setup=True):
            output = _run([sys.executable, str(Path(__file__).with_name("v1_evaluator.py")),
                           "prepare" if phase == "setup" else "evaluate", "--upstream", self.config["upstream_checkout"],
                           "--task-id", CHROME_TASK, "--episode-dir", str(self.directory),
                           "--endpoint", f"http://127.0.0.1:{self.config['setup_local_port']}"],
                          env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
        print(output.rstrip())

    def action(self) -> None:
        """One AUV screenshot; no GUI input, no agent, no task-solving claim."""
        self.assert_identity()
        sidecar = Path(os.environ["AUV_OSWORLD_ACTION_EVIDENCE"])
        if sidecar != self.directory / "action_evidence.json":
            raise ValueError("action evidence path must be the runner episode sidecar")
        paired = json.loads((self.directory / "paired-device.json").read_text())
        profile_path = self.directory / "paired-profiles.json"
        if not profile_path.exists():
            raise ValueError("paired credential file missing")
        env = {**os.environ, "AUV_CONFIG_PROFILES_FILE": str(profile_path),
               "AUV_DISCOVERY_FILE": str(self.directory / "no-local-discovery.json")}
        with self.forward(auv=True):
            raw = _run([self.config["host_auv_binary"], "--device", paired["device_id"], "invoke",
                        "display.capture", "--json", "--store-root", str(self.directory / "auv-runs")], env=env)
            capture = json.loads(raw)
            if not isinstance(capture.get("run_id"), str) or not capture["run_id"]:
                raise ValueError("AUV capture returned no Run ID")
            artifacts = capture.get("artifacts", [])
            png = [item for item in artifacts if item.get("purpose") == "auv.driver.display_capture" and item.get("file_path")]
            if len(png) != 1:
                raise ValueError("AUV capture returned no unique PNG artifact")
            source = Path(png[0]["file_path"]).resolve(strict=True)
            target = self.directory / "final-screenshot.png"
            shutil.copyfile(source, target)
            evidence = {"run_ids": [capture["run_id"]], "final_artifact": {"path": target.name, "sha256": sha256(target)}}
            write_json(sidecar, evidence)
        self.assert_identity()
        print(json.dumps(evidence))

    def reset(self) -> None:
        # Local access is revoked first, including when ownership evidence is
        # corrupt and cluster cleanup must stop for manual inspection.
        (self.directory / "paired-profiles.json").unlink(missing_ok=True)
        removed = []
        owned = json.loads(self.owned_path.read_text()) if self.owned_path.exists() else []
        expected = {("pod", self.config["runtime_pod"]), ("service", self.config["runtime_service"]),
                    ("pod", self.config["proxy_pod"])}
        if not isinstance(owned, list) or len(owned) > len(expected):
            raise ValueError("invalid ownership journal")
        seen = set()
        for item in owned:
            if not isinstance(item, dict) or (item.get("kind"), item.get("name")) not in expected or \
               (item["kind"], item["name"]) in seen or not isinstance(item.get("uid"), str) or not item["uid"]:
                raise ValueError("ownership journal contains an unapproved or duplicate resource")
            seen.add((item["kind"], item["name"]))
        try:
            with (self.api_proxy() if owned else nullcontext(None)) as origin:
                for item in reversed(owned):
                    observed = self.get(item["kind"], item["name"])
                    role = "qemu" if item["name"] == self.config["runtime_pod"] else "proxy" if item["name"] == self.config["proxy_pod"] else None
                    if observed["metadata"].get("uid") != item["uid"] or observed["metadata"].get("labels") != self._labels(role):
                        raise ValueError(f"refusing to delete replaced {item['kind']}/{item['name']}")
                    self.delete_owned(origin, item)
                    removed.append({"kind": item["kind"], "name": item["name"], "uid": item["uid"]})
            retained = self._retained_pvc()
            if self.identity_path.exists() and retained != json.loads(self.identity_path.read_text())["retained_pvc"]:
                raise ValueError("retained hot PVC/PV identity changed")
        finally:
            # The paired bearer is scoped to this episode and must not survive
            # a failed cluster cleanup in the local harness account.
            (self.directory / "paired-profiles.json").unlink(missing_ok=True)
        print(json.dumps({"removed_resources": removed, "retained_pvcs_verified": [retained]}))

    def _retained_pvc(self) -> dict:
        pvc = self.get("pvc", self.config["base_pvc"])
        if pvc["status"].get("phase") != "Bound":
            raise ValueError("retained hot PVC is not Bound")
        pv = self.get("pv", pvc["spec"]["volumeName"])
        terms = pv["spec"]["nodeAffinity"]["required"]["nodeSelectorTerms"]
        if not any(expr.get("key") == "kubernetes.io/hostname" and self.config["node"] in expr.get("values", [])
                   for term in terms for expr in term.get("matchExpressions", [])):
            raise ValueError("retained hot PVC node affinity changed")
        return {"name": self.config["base_pvc"], "uid": pvc["metadata"]["uid"], "pv_uid": pv["metadata"]["uid"]}


def main() -> None:
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("manifest")
    build.add_argument("--config", type=Path, required=True)
    phase = sub.add_parser("phase")
    phase.add_argument("phase", choices=PHASES)
    phase.add_argument("--config", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "manifest":
        print(json.dumps(manifest(args.config), sort_keys=True, indent=2))
        return
    config = load_config(args.config)
    directory = Path(os.environ["AUV_OSWORLD_EPISODE_DIR"]).resolve(strict=True)
    if directory.name != config["episode_id"]:
        raise ValueError("runner episode directory and config ID differ")
    episode = Episode(config, directory)
    getattr(episode, args.phase if args.phase not in ("setup", "evaluate") else "evaluator")(
        *([args.phase] if args.phase in ("setup", "evaluate") else []))


if __name__ == "__main__":
    main()
