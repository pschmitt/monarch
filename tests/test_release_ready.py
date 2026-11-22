import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/release-ready.sh"


class ReleaseReadyTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        gh = self.bin / "gh"
        gh.write_text('#!/bin/sh\nprintf "%s\\n" "$FAKE_STATUS_COUNT"\n')
        gh.chmod(0o755)
        subprocess.run(["git", "init", "-q", str(self.root / "repo")], check=True)
        self.repo = self.root / "repo"
        subprocess.run(["git", "-C", str(self.repo), "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                        "commit", "-qm", "initial", "--allow-empty"], check=True)
        self.sha = subprocess.check_output(["git", "-C", str(self.repo), "rev-parse", "HEAD"], text=True).strip()
        subprocess.run(["git", "-C", str(self.repo), "tag", "v1.2.3"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                        "tag", "-a", "v1.2.4", "-m", "annotated"], check=True)

    def run_gate(self, event, count="1", sha=None):
        output = self.root / "output"
        output.write_text("")
        env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}", "EVENT_NAME": event,
               "GITHUB_REPOSITORY": "owner/repo", "GITHUB_SHA": sha or self.sha,
               "GITHUB_REF_NAME": "v1.2.3", "GITHUB_OUTPUT": str(output), "FAKE_STATUS_COUNT": count}
        subprocess.run(["bash", str(SCRIPT)], cwd=self.repo, env=env, check=True)
        result = dict(line.split("=", 1) for line in output.read_text().splitlines())
        return result["ready"], json.loads(result["tags"])

    def test_tag_waits_without_successful_circus_status(self):
        self.assertEqual(self.run_gate("push", "0"), ("false", []))
        self.assertEqual(self.run_gate("push", "1"), ("true", ["v1.2.3"]))

    def test_status_releases_only_tags_pointing_at_that_commit(self):
        self.assertEqual(self.run_gate("status"), ("true", ["v1.2.3", "v1.2.4"]))
        subprocess.run(["git", "-C", str(self.repo), "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                        "commit", "-qm", "next", "--allow-empty"], check=True)
        next_sha = subprocess.check_output(["git", "-C", str(self.repo), "rev-parse", "HEAD"], text=True).strip()
        self.assertEqual(self.run_gate("status", sha=next_sha), ("false", []))


if __name__ == "__main__":
    unittest.main()
