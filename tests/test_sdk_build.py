"""Fast, offline failure controls for the native SDK build entry point."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'build-sdk.py'


class BuildControls(unittest.TestCase):
    def invoke(self, *args, env=None):
        return subprocess.run([sys.executable, str(SCRIPT), *map(str, args)],
                              cwd='/', env=env, text=True, capture_output=True, timeout=20)

    def test_help_needs_no_build_tools(self):
        result = self.invoke('--help', env={**os.environ, 'PATH': ''})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('--cache', result.stdout)

    def test_missing_prerequisite_does_not_create_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'output'
            result = self.invoke('--output', output, env={**os.environ, 'PATH': ''})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Missing prerequisite: git', result.stderr)
            self.assertFalse(output.exists())

    def test_existing_output_is_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'output'
            output.mkdir()
            sentinel = output / 'sentinel'
            sentinel.write_text('keep')
            result = self.invoke('--output', output)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Output already exists', result.stderr)
            self.assertEqual(sentinel.read_text(), 'keep')

    def test_corrupt_tool_cache_cannot_produce_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tools = root / 'cache/tools'
            tools.mkdir(parents=True)
            (tools / 'wasi-sdk-27.0-x86_64-linux.tar.gz').write_bytes(b'corrupted')
            # Bypass toolchain installation only; exercise the real cache verifier.
            fake_bin = root / 'bin'
            fake_bin.mkdir()
            for name, body in [('rustup', 'exit 0'), ('node', 'echo 22')]:
                executable = fake_bin / name
                executable.write_text('#!/bin/sh\n' + body + '\n')
                executable.chmod(0o755)
            result = self.invoke('--output', root / 'output', '--cache', root / 'cache',
                                 env={**os.environ, 'PATH': str(fake_bin) + os.pathsep + os.environ['PATH']})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Cached archive checksum mismatch', result.stderr)
            self.assertFalse((root / 'output').exists())


if __name__ == '__main__':
    unittest.main()
