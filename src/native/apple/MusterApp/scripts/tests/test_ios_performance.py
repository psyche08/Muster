import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('ios_performance', Path(__file__).parents[1] / 'ios-performance.py')
monitor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(monitor)


class PerformanceSummaryTests(unittest.TestCase):
    def test_deduplication_page_grouping_and_partial_tail(self):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root)
            a = dict(timestamp=100, run_id='a', cpu_percent=3.0, screen=5, thermal_state=0)
            b = dict(timestamp=110, run_id='a', cpu_percent=25.0, screen=6, thermal_state=1)
            (directory / 'performance.previous.jsonl').write_text(json.dumps(a) + '\n')
            (directory / 'performance.jsonl').write_text(json.dumps(a) + '\n' + json.dumps(b) + '\n{"partial":')
            result = monitor.summarize(directory)
            self.assertEqual(result['samples'], 2)
            self.assertEqual(result['incomplete_tail_records_ignored'], 1)
            self.assertEqual(result['screens']['6']['cpu_median'], 25.0)
            self.assertEqual(result['screens']['5']['thermal_max'], 0)
            self.assertEqual(monitor.summarize(directory, .1)['samples'], 1)

    def test_exported_file_and_invalid_complete_record(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'Muster-performance.jsonl'
            path.write_text(json.dumps(dict(timestamp=100, cpu_percent=1, screen=6, thermal_state=0)) + '\n')
            self.assertEqual(monitor.summarize(path)['samples'], 1)
            path.write_text('{broken}\n')
            with self.assertRaises(json.JSONDecodeError):
                monitor.summarize(path)

    def test_uploaded_names_and_stack_capture_counts(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'iPhone-20261006-performance.jsonl'
            row = dict(timestamp=100, cpu_percent=180, screen=8, thermal_state=1,
                       high_cpu_stacks={'samples': [{'frames': []}, {'frames': []}]})
            path.write_text(json.dumps(row) + '\n')
            result = monitor.summarize(Path(root))
            self.assertEqual(result['samples'], 1)
            self.assertEqual(result['stack_captures'], 1)
            self.assertEqual(result['stack_samples'], 2)

    def test_empty_directory(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertEqual(monitor.summarize(Path(root))['samples'], 0)


if __name__ == '__main__':
    unittest.main()
