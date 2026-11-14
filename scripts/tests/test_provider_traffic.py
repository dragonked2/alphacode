import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("provider_traffic", Path(__file__).resolve().parents[1] / "provider_traffic.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class TrafficTests(unittest.TestCase):
    def test_empty(self):
        report = module.summarize([])
        self.assertEqual(report["total"], 0)
        self.assertEqual(report["peak_rolling_60s"], 0)

    def test_counts_rounds_retries_background_and_minute_boundaries(self):
        events = [dict(unix_ms=stamp, session=session, prompt=prompt, kind=kind, origin="https://fixture.invalid", method="POST")
                  for stamp, session, prompt, kind in [
                      (0, None, None, "models"),
                      (1000, "task-a", "one", "inference_or_other_post"),
                      (2000, "task-a", "two", "inference_or_other_post"),
                      (3000, "task-a", "two", "inference_or_other_post"),
                      (60000, "task-b", "three", "inference_or_other_post"),
                  ]]
        report = module.summarize(reversed(events))
        self.assertEqual(report["total"], 5)
        self.assertEqual(report["by_prompt"], {"task-a/one": 1, "task-a/two": 2, "task-b/three": 1})
        self.assertEqual(report["by_session"], {"task-a": 3, "task-b": 1})
        self.assertEqual(report["unattributed"], 1)
        self.assertEqual(report["peak_rolling_60s"], 4)
        self.assertEqual(list(report["by_utc_calendar_minute"].values()), [4, 1])

    def test_websocket_messages_are_separate_from_http_handshake(self):
        events = [dict(unix_ms=stamp, session="task", prompt="turn", kind=kind,
                       origin="wss://fixture.invalid", method=method)
                  for stamp, method, kind in [(0, "GET", "websocket_connect"),
                                               (1, "WS", "websocket_inference"),
                                               (2, "WS", "websocket_inference")]]
        report = module.summarize(events)
        self.assertEqual(report["total"], 3)
        self.assertEqual(report["http_dispatches"], 1)
        self.assertEqual(report["websocket_inference_messages"], 2)


if __name__ == "__main__":
    unittest.main()
