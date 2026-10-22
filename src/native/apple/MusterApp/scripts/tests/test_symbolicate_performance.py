import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location('symbolicate', Path(__file__).parents[1] / 'symbolicate-performance.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def report():
    return {'high_cpu_stacks': {'images': {'a': {'uuid': 'ABC', 'arch': 'arm64', 'base': '0x1000', 'size': 4096}},
                               'samples': [{'frames': [{'pc': '0x1100', 'kind': 'return', 'image': 'a'}]}]}}


class SymbolicateTests(unittest.TestCase):
    def test_wrong_uuid_never_uses_symbols_from_another_build(self):
        with patch.object(module.subprocess, 'run') as run:
            row = module.symbolicate(report(), {('different', 'arm64'): Path('/wrong')})
            self.assertEqual(row['high_cpu_stacks']['samples'][0]['frames'][0]['symbolication'], 'missing_matching_dsym')
            run.assert_not_called()

    def test_return_address_and_aslr_base_are_passed_to_atos(self):
        with patch.object(module.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout='rust::hot (hot.rs:12)')) as run:
            row = module.symbolicate(report(), {('abc', 'arm64'): Path('/matching')})
            self.assertEqual(run.call_args.args[0][-3:], ['-l', '0x1000', '0x10ff'])
            self.assertEqual(row['high_cpu_stacks']['samples'][0]['frames'][0]['symbol'], 'rust::hot (hot.rs:12)')

    def test_unresolved_and_invalid_addresses_stay_explicit(self):
        with patch.object(module.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout='0x10ff')):
            row = module.symbolicate(report(), {('abc', 'arm64'): Path('/matching')})
            self.assertEqual(row['high_cpu_stacks']['samples'][0]['frames'][0]['symbolication'], 'unresolved')
        row = report()
        row['high_cpu_stacks']['samples'][0]['frames'][0]['pc'] = '0x9999'
        with patch.object(module.subprocess, 'run') as run:
            module.symbolicate(row, {('abc', 'arm64'): Path('/matching')})
            run.assert_not_called()
            self.assertEqual(row['high_cpu_stacks']['samples'][0]['frames'][0]['symbolication'], 'invalid_image_range')


if __name__ == '__main__':
    unittest.main()
