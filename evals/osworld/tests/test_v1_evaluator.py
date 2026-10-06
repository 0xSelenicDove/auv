"""Boundary tests; no guest or GUI input is used."""

import ast
import importlib.util
import json
import logging
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


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
