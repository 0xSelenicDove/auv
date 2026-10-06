"""Boundary tests; no guest or GUI input is used."""

import ast
from contextlib import redirect_stdout
import importlib.util
import io
import json
import logging
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch
from urllib.parse import urlsplit


BRIDGE_PATH = Path(__file__).resolve().parents[1] / "v1_evaluator.py"
spec = importlib.util.spec_from_file_location("v1_evaluator", BRIDGE_PATH)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class FakeController:
    def __init__(self, **kwargs):
        self.options = kwargs

    def click(self, *args):
        raise AssertionError("GUI input must not pass through the OSWorld controller")


class FakeSetupController:
    def __init__(self, **kwargs):
        self.options = kwargs
        self.calls = []

    def reset_cache_dir(self, directory):
        self.cache_dir = directory

    def setup(self, config, use_proxy):
        self.calls.append((config, use_proxy))
        return True


class FakeDesktopEnv:
    def _set_task_info(self, task):
        self.task_id = task["id"]
        self.cache_dir = str(Path(self.cache_dir_base) / self.task_id)
        Path(self.cache_dir).mkdir(parents=True, exist_ok=True)
        self.config = task["config"]
        self.evaluator = task["evaluator"]

    def step(self, *args):
        raise AssertionError("DesktopEnv.step would deliver PyAutoGUI input")

    def evaluate(self):
        self.setup_controller.setup(self.evaluator.get("postconfig", []), self.enable_proxy)
        return 0.75


class BoundaryTest(unittest.TestCase):
    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_vlc_rejects_mutated_task_before_binding_guest_commands(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task, _ = bridge.load_task(upstream, "5ac2891a-eacd-4954-b339-98abba077adb")
        task["config"][1]["parameters"]["command"] = ["python", "-c", "import pyautogui; pyautogui.click(10, 10)"]
        fake_requests = types.SimpleNamespace(
            get=lambda *_args, **_kwargs: self.fail("no guest request expected"),
            post=lambda *_args, **_kwargs: self.fail("no guest request expected"),
            exceptions=types.SimpleNamespace(ReadTimeout=TimeoutError, RequestException=OSError),
        )
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, {"requests": fake_requests}), \
                self.assertRaisesRegex(ValueError, "pinned VLC task"):
            bridge.external_env(task, Path(directory), "127.0.0.1", 5000, 9222, upstream)

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_vlc_rejects_unpinned_revision_and_task_bytes(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task_id = "5ac2891a-eacd-4954-b339-98abba077adb"
        with patch.object(bridge.subprocess, "check_output", return_value="different revision\n"):
            with self.assertRaisesRegex(ValueError, "not pinned"):
                bridge.load_task(upstream, task_id)
        with patch.object(Path, "read_bytes", return_value=b"{}"):
            with self.assertRaisesRegex(ValueError, "task JSON SHA256"):
                bridge.load_task(upstream, task_id)

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_vlc_prepare_rejects_http_200_with_failed_guest_command(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task, _ = bridge.load_task(upstream, "5ac2891a-eacd-4954-b339-98abba077adb")
        secret = "response-only-sensitive-sentinel"

        class Response:
            status_code = 200
            text = "VLC_VERBOSE=-1 vlc --no-audio --no-video-title-show launched successfully"

            def json(self):
                return {"status": "success", "output": secret, "error": secret, "returncode": 1}

        def post(url, **_kwargs):
            self.assertIn(url.rsplit("/", 1)[-1], ("launch", "execute"))
            return Response()

        fake_requests = types.SimpleNamespace(
            get=lambda *_args, **_kwargs: Response(), post=post,
            exceptions=types.SimpleNamespace(ReadTimeout=TimeoutError, RequestException=OSError),
        )
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, {"requests": fake_requests}):
            env = bridge.external_env(task, Path(directory), "127.0.0.1", 5000, 9222, upstream)
            with self.assertLogs("desktopenv.evaluator-only", level="ERROR") as logs, \
                    self.assertRaisesRegex(Exception, "VLC guest command did not complete successfully") as caught:
                bridge.prepare(env)
            self.assertNotIn(secret, str(caught.exception))
            self.assertNotIn(secret, "\n".join(logs.output))

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_vlc_prepare_rejects_missing_guest_file_postcondition(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task, _ = bridge.load_task(upstream, "5ac2891a-eacd-4954-b339-98abba077adb")

        class Response:
            status_code = 200
            content = b"play-and-exit=0\n"
            text = "VLC_VERBOSE=-1 vlc --no-audio --no-video-title-show launched successfully"

            def __init__(self, output=""):
                self.output = output

            def json(self):
                return {"status": "success", "output": self.output, "error": "", "returncode": 0}

        def post(url, **kwargs):
            if urlsplit(url).path == "/execute":
                command = json.loads(kwargs["data"])["command"][-1]
                return Response("Linux\n" if command.endswith("import platform; print(platform.system())")
                                else "/home/user/.config/vlc/vlcrc\n")
            return Response()

        fake_requests = types.SimpleNamespace(
            get=lambda *_args, **_kwargs: Response(), post=post,
            exceptions=types.SimpleNamespace(ReadTimeout=TimeoutError, RequestException=OSError),
        )
        with patch.dict(sys.modules, {"requests": fake_requests}):
            for file_bytes in (b"play-and-exit=0\n", b"#play-and-exit=1\n"):
                with self.subTest(file_bytes=file_bytes), tempfile.TemporaryDirectory() as directory:
                    Response.content = file_bytes
                    env = bridge.external_env(task, Path(directory), "127.0.0.1", 5000, 9222, upstream)
                    with self.assertRaisesRegex(RuntimeError, "did not establish play-and-exit=1"):
                        bridge.prepare(env)

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_vlc_setup_does_not_echo_malformed_guest_json(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task, _ = bridge.load_task(upstream, "5ac2891a-eacd-4954-b339-98abba077adb")
        secret = "response-only-sensitive-sentinel"

        class Response:
            status_code = 200
            text = "VLC_VERBOSE=-1 vlc --no-audio --no-video-title-show launched successfully"

            def json(self):
                raise ValueError(secret)

        fake_requests = types.SimpleNamespace(
            get=lambda *_args, **_kwargs: Response(), post=lambda *_args, **_kwargs: Response(),
            exceptions=types.SimpleNamespace(ReadTimeout=TimeoutError, RequestException=OSError),
        )
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, {"requests": fake_requests}):
            env = bridge.external_env(task, Path(directory), "127.0.0.1", 5000, 9222, upstream)
            with self.assertLogs("desktopenv.evaluator-only", level="ERROR") as logs, \
                    self.assertRaisesRegex(Exception, "invalid JSON") as caught:
                bridge.prepare(env)
            self.assertNotIn(secret, str(caught.exception))
            self.assertNotIn(secret, "\n".join(logs.output))

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_vlc_prepare_rejects_wrong_guest_file_even_if_it_contains_expected_setting(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task, _ = bridge.load_task(upstream, "5ac2891a-eacd-4954-b339-98abba077adb")

        class Response:
            status_code = 200
            content = b"play-and-exit=1\n"
            text = "VLC_VERBOSE=-1 vlc --no-audio --no-video-title-show launched successfully"

            def __init__(self, output=""):
                self.output = output

            def json(self):
                return {"status": "success", "output": self.output, "error": "", "returncode": 0}

        posts = []

        def post(url, **kwargs):
            posts.append(url)
            if urlsplit(url).path == "/execute":
                command = json.loads(kwargs["data"])["command"][-1]
                return Response("Linux\n" if command.endswith("import platform; print(platform.system())")
                                else "/tmp/alternate/vlcrc\n")
            return Response()

        fake_requests = types.SimpleNamespace(
            get=lambda *_args, **_kwargs: Response(), post=post,
            exceptions=types.SimpleNamespace(ReadTimeout=TimeoutError, RequestException=OSError),
        )
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, {"requests": fake_requests}):
            env = bridge.external_env(task, Path(directory), "127.0.0.1", 5000, 9222, upstream)
            env.controller.retry_times = 1
            env.controller.retry_interval = 0
            with self.assertLogs("desktopenv.evaluator-only", level="ERROR"), \
                    self.assertRaisesRegex(RuntimeError, "VLC setup file verification failed"):
                bridge.prepare(env)
            self.assertFalse(any(url.endswith("/file") for url in posts), "wrong path must not reach file transport")

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_vlc_evaluate_requires_matching_prepare_marker_and_endpoint(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task_id = "5ac2891a-eacd-4954-b339-98abba077adb"
        _, task_hash = bridge.load_task(upstream, task_id)
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "prepared.json"
            marker.write_text(json.dumps({
                "boundary": "evaluator-only; GUI actions must use AUV",
                "upstream_revision": bridge.UPSTREAM_REV,
                "task_id": task_id,
                "task_sha256": task_hash,
                "endpoint": "http://127.0.0.1:5000",
                "chromium_port": 9222,
            }))
            argv = ["v1_evaluator.py", "evaluate", "--upstream", str(upstream), "--task-id", task_id,
                    "--episode-dir", directory, "--endpoint", "http://127.0.0.1:5001"]
            with patch.object(sys, "argv", argv), self.assertRaisesRegex(ValueError, "matching prepare marker"):
                bridge.main()
            self.assertEqual(json.loads(marker.read_text())["endpoint"], "http://127.0.0.1:5000")

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_pinned_vlc_prepare_sets_negative_control_and_evaluates_without_gui_relay(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task, task_hash = bridge.load_task(upstream, "5ac2891a-eacd-4954-b339-98abba077adb")
        self.assertEqual(task_hash, "4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da")
        self.assertEqual(task["snapshot"], "base_setup")
        guest = {"vlcrc": b"play-and-exit=0\n"}
        calls = []

        class Response:
            status_code = 200

            def __init__(self, payload=None, content=b""):
                self.payload = payload or {"status": "success", "output": "", "error": "", "returncode": 0}
                self.content = content
                self.text = json.dumps(self.payload)

            def json(self):
                return self.payload

        def get(url, **_kwargs):
            self.assertTrue(url.endswith("/terminal"))
            return Response()

        def post(url, **kwargs):
            calls.append(url.rsplit("/", 1)[-1])
            if url.endswith("/setup/launch"):
                command = json.loads(kwargs["data"])["command"]
                self.assertEqual(command, task["config"][0]["parameters"]["command"])
                response = Response()
                response.text = f"{command} launched successfully"
                return response
            if url.endswith("/setup/execute"):
                command = json.loads(kwargs["data"])["command"]
                self.assertEqual(command, task["config"][1]["parameters"]["command"])
                guest["vlcrc"] = b"play-and-exit=1\n"
                return Response()
            if url.endswith("/execute"):
                command = json.loads(kwargs["data"])["command"][-1]
                self.assertNotIn("pyautogui.click", command)
                self.assertNotIn("pyautogui.keyDown", command)
                if command.endswith("import platform; print(platform.system())"):
                    return Response({"status": "success", "output": "Linux\n", "error": "", "returncode": 0})
                self.assertIn("expanduser('~/.config/vlc/vlcrc')", command)
                return Response({"status": "success", "output": "/home/user/.config/vlc/vlcrc\n", "error": "", "returncode": 0})
            if url.endswith("/file"):
                self.assertEqual(kwargs["data"]["file_path"], "/home/user/.config/vlc/vlcrc")
                return Response(content=guest["vlcrc"])
            self.fail(f"unreviewed route: {url}")

        fake_requests = types.SimpleNamespace(
            get=get, post=post,
            exceptions=types.SimpleNamespace(ReadTimeout=TimeoutError, RequestException=RuntimeError),
        )
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, {"requests": fake_requests}):
            env = bridge.external_env(task, Path(directory), "127.0.0.1", 5000, 9222, upstream)
            self.assertEqual(env.evaluate.__code__.co_filename, str(upstream / "desktop_env" / "desktop_env.py"))
            bridge.prepare(env)
            self.assertEqual(guest["vlcrc"], b"play-and-exit=1\n")
            self.assertEqual(bridge.evaluate(env), 0.0)
            before = list(calls)
            env.controller.retry_times = 1
            env.controller.retry_interval = 0
            with self.assertLogs("desktopenv.evaluator-only", level="ERROR"):
                self.assertIsNone(env.controller.execute_python_command("pyautogui.click(10, 10)"))
            self.assertEqual(calls, before, "unreviewed GUI command must not reach the guest")
        self.assertEqual(calls.count("launch"), 1)
        self.assertEqual(calls.count("execute"), 5)
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, {"requests": fake_requests}), redirect_stdout(io.StringIO()) as output:
            argv = ["v1_evaluator.py", "prepare", "--upstream", str(upstream), "--task-id", task["id"],
                    "--episode-dir", directory, "--endpoint", "http://127.0.0.1:5000"]
            with patch.object(sys, "argv", argv):
                bridge.main()
            marker = json.loads((Path(directory) / "prepared.json").read_text())
            self.assertEqual(marker["task_sha256"], task_hash)
            self.assertEqual(marker["endpoint"], "http://127.0.0.1:5000")
            argv[1] = "evaluate"
            with patch.object(sys, "argv", argv):
                bridge.main()
            self.assertEqual(json.loads(output.getvalue().splitlines()[-1])["score"], 0.0)

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_pinned_chrome_method_chain_scores_negative_control_without_gui_input(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        task, _ = bridge.load_task(upstream, "2ad9387a-65d8-4e33-ad5b-7580065a27ca")
        calls = []

        class Response:
            status_code = 200
            content = b'{"roots":{"bookmark_bar":{"children":[]}}}'
            text = '{"status":"success"}'

            def __init__(self, output=""):
                self.output = output

            def json(self):
                return {"status": "success", "output": self.output}

        def get(url, **_kwargs):
            calls.append(("GET", url))
            self.assertTrue(url.endswith("/terminal"))
            return Response()

        def post(url, **kwargs):
            calls.append(("POST", url))
            if url.endswith("/execute"):
                command = json.loads(kwargs["data"])["command"][-1]
                self.assertNotIn("pyautogui.click", command)
                if "platform.system()" in command:
                    return Response("Linux\n")
                if "platform.machine()" in command:
                    return Response("x86_64\n")
                self.assertIn("Bookmarks", command)
                return Response("/home/user/.config/google-chrome/Default/Bookmarks\n")
            return Response()

        fake_requests = types.SimpleNamespace(
            get=get, post=post,
            exceptions=types.SimpleNamespace(ReadTimeout=TimeoutError, RequestException=RuntimeError),
        )
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, {"requests": fake_requests}):
            env = bridge.external_env(task, Path(directory), "127.0.0.1", 5000, 9222, upstream)
            self.assertEqual(env.evaluate.__code__.co_filename, str(upstream / "desktop_env" / "desktop_env.py"))
            bridge.prepare(env)
            self.assertEqual(bridge.evaluate(env), 0.0)
        self.assertEqual(sum(url.endswith("/setup/launch") for _, url in calls), 4)
        self.assertEqual(sum(url.endswith("/file") for _, url in calls), 1)

    @unittest.skipUnless(os.getenv("OSWORLD_V1_CHECKOUT"), "set OSWORLD_V1_CHECKOUT for pinned upstream source contract")
    def test_exact_pinned_evaluate_method_handles_postconfig_getter_and_metric(self):
        upstream = Path(os.environ["OSWORLD_V1_CHECKOUT"])
        tree = ast.parse((upstream / "desktop_env" / "desktop_env.py").read_text())
        desktop_class = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "DesktopEnv")
        method = next(node for node in desktop_class.body if isinstance(node, ast.FunctionDef) and node.name == "evaluate")
        module = ast.fix_missing_locations(ast.Module(body=[method], type_ignores=[]))
        namespace = {"logger": logging.getLogger("v1-evaluator-contract")}
        exec(compile(module, str(upstream / "desktop_env" / "desktop_env.py"), "exec"), namespace)

        with tempfile.TemporaryDirectory() as directory:
            env = FakeDesktopEnv.__new__(FakeDesktopEnv)
            env.setup_controller = FakeSetupController()
            env.enable_proxy = False
            env.is_environment_used = False
            env.action_history = []
            env.evaluator = {
                "postconfig": [{"type": "sleep", "parameters": {"seconds": 3}}],
                "func": "metric",
                "result": {"type": "bookmarks"},
                "expected": {"type": "rule"},
            }
            env.result_getter = lambda _env, _config: {"bookmark_bar": {"children": [{"name": "Favorites"}]}}
            env.expected_getter = lambda _env, _config: {"names": ["Favorites"]}
            env.metric = lambda result, rule, **_options: float(result["bookmark_bar"]["children"][0]["name"] == rule["names"][0])
            env.metric_options = {}
            env.cache_dir = directory
            self.assertEqual(namespace["evaluate"](env), 1.0)
            self.assertEqual(env.setup_controller.calls, [(env.evaluator["postconfig"], False)])
            self.assertTrue(env.is_environment_used)


if __name__ == "__main__":
    unittest.main()
