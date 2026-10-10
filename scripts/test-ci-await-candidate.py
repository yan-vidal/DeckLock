#!/usr/bin/env python3
"""Exercise the candidate awaiter's real CLI against a fake `gh`, never GitHub."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent
AWAITER = ROOT / "scripts/ci-await-candidate"
SCRATCH = ROOT / "target/dev-ci-await"
JOB = "Package (Ubuntu aarch64) / Ubuntu aarch64 package contract"
ARTIFACT = "ubuntu-package-aarch64"

# Serves a scripted sequence of job listings (the last one repeats) and copies a
# prepared artifact on download, recording every invocation. PATH holds nothing
# else, so the interpreter is named absolutely and no real gh can be reached.
FAKE_GH = '#!' + sys.executable + '\n' + textwrap.dedent('''\
    import json
    import os
    from pathlib import Path
    import shutil
    import sys

    state = Path(os.environ["FAKE_GH_STATE"])
    with (state / "calls").open("a") as log:
        log.write(json.dumps(sys.argv[1:]) + "\\n")
    if sys.argv[1] == "api":
        responses = sorted((state / "jobs").iterdir())
        counter = state / "counter"
        index = int(counter.read_text()) if counter.exists() else 0
        counter.write_text(str(index + 1))
        text = responses[min(index, len(responses) - 1)].read_text()
        if text.startswith("ERROR"):
            sys.stderr.write(text)
            sys.exit(1)
        sys.stdout.write(text)
        sys.exit(0)
    if sys.argv[1:3] == ["run", "download"]:
        directory = Path(sys.argv[sys.argv.index("--dir") + 1])
        shutil.copytree(state / "artifact", directory, dirs_exist_ok=True)
        sys.exit(0)
    sys.exit(64)
''')


def listing(*jobs):
    return "".join(f"{name}\t{status}\t{conclusion}\n" for name, status, conclusion in jobs)


class AwaitCandidate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)

    def run_awaiter(self, listings, *, files=None, sums=None, environment=None, arguments=(),
                    timeout="5", job=JOB, prepare=None, without_gh=False):
        with tempfile.TemporaryDirectory(prefix="await-", dir=SCRATCH) as directory:
            work = Path(directory)
            tools = work / "bin"
            tools.mkdir()
            if not without_gh:
                (tools / "gh").write_text(FAKE_GH)
                (tools / "gh").chmod(0o755)
            state = work / "state"
            (state / "jobs").mkdir(parents=True)
            for index, text in enumerate(listings):
                (state / "jobs" / f"{index:03}.tsv").write_text(text)
            artifact = state / "artifact"
            artifact.mkdir()
            files = {"decklock_0.0.0-1_arm64.deb": b"candidate"} if files is None else files
            for name, content in files.items():
                (artifact / name).write_bytes(content)
            if sums is None:
                sums = "".join(f"{hashlib.sha256(content).hexdigest()}  {name}\n"
                               for name, content in files.items())
            if sums is not False:
                (artifact / "SHA256SUMS").write_text(sums)
            destination = work / "dist"
            if prepare:
                prepare(destination)
            env = {"PATH": str(tools), "FAKE_GH_STATE": str(state), "GH_TOKEN": "fixture-token",
                   "GITHUB_REPOSITORY": "fixture/decklock", "GITHUB_RUN_ID": "4242",
                   "GITHUB_RUN_ATTEMPT": "3", "HOME": str(work / "home"),
                   "PYTHONDONTWRITEBYTECODE": "1"}
            env.update(environment or {})
            env = {key: value for key, value in env.items() if value is not None}
            started = time.monotonic()
            result = subprocess.run(
                [sys.executable, "-B", str(AWAITER), "--job", job, "--artifact", ARTIFACT,
                 "--directory", str(destination), "--timeout", timeout, "--interval", "0.05",
                 *arguments],
                cwd=work, env=env, capture_output=True, text=True, timeout=30)
            calls = [json.loads(line) for line in (state / "calls").read_text().splitlines()] \
                if (state / "calls").exists() else []
            received = {path.name: path.read_bytes() for path in destination.iterdir()} \
                if destination.is_dir() else {}
            return result, calls, received, time.monotonic() - started

    def downloads(self, calls):
        return [call for call in calls if call[:2] == ["run", "download"]]

    def test_waits_for_success_in_this_attempt_then_downloads_and_verifies(self):
        result, calls, received, _ = self.run_awaiter([
            listing(("DeckLock isolated gates", "in_progress", "")),
            listing((JOB, "queued", "")),
            listing((JOB, "in_progress", "")),
            listing((JOB, "completed", "success")),
        ])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(received.get("decklock_0.0.0-1_arm64.deb"), b"candidate")
        polls = [call for call in calls if call[0] == "api"]
        self.assertGreaterEqual(len(polls), 4)
        for poll in polls:
            self.assertTrue(any("repos/fixture/decklock/actions/runs/4242/attempts/3/jobs" in part
                                for part in poll), poll)
        downloads = self.downloads(calls)
        self.assertEqual(len(downloads), 1, calls)
        self.assertIn("4242", downloads[0])
        self.assertEqual(downloads[0][downloads[0].index("--name") + 1], ARTIFACT)
        self.assertEqual(downloads[0][downloads[0].index("--repo") + 1], "fixture/decklock")
        # Nothing is fetched while the job is still running: an artifact left by an
        # earlier attempt is not this attempt's candidate.
        self.assertGreaterEqual(calls.index(downloads[0]), 4)
        self.assertEqual(calls[-1], downloads[0])

    def test_a_job_that_did_not_succeed_ends_the_wait_without_a_download(self):
        for conclusion in ("failure", "cancelled", "skipped", "timed_out"):
            with self.subTest(conclusion=conclusion):
                result, calls, received, elapsed = self.run_awaiter(
                    [listing((JOB, "in_progress", "")), listing((JOB, "completed", conclusion))],
                    timeout="20")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(conclusion, result.stderr)
                self.assertEqual(self.downloads(calls), [])
                self.assertEqual(received, {})
                self.assertLess(elapsed, 10, "A failed job must not be waited out")

    def test_the_wait_is_bounded(self):
        result, calls, received, elapsed = self.run_awaiter(
            [listing((JOB, "in_progress", ""))], timeout="0.6")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("0.6", result.stderr)
        self.assertEqual(self.downloads(calls), [])
        self.assertLess(elapsed, 10)

    def test_only_the_named_job_counts_also_when_a_caller_prefixes_it(self):
        other = "Package (Ubuntu) / Ubuntu package contract"
        result, calls, _, _ = self.run_awaiter(
            [listing((other, "completed", "success"), (JOB, "in_progress", ""))], timeout="0.6")
        self.assertNotEqual(result.returncode, 0, "Another target's package is not this candidate")
        self.assertEqual(self.downloads(calls), [])
        result, calls, received, _ = self.run_awaiter(
            [listing((other, "completed", "failure"), ("quality / " + JOB, "completed", "success"))])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.downloads(calls)), 1)
        self.assertIn("decklock_0.0.0-1_arm64.deb", received)

    def test_two_jobs_with_the_name_are_refused(self):
        result, calls, _, _ = self.run_awaiter(
            [listing((JOB, "completed", "success"), ("quality / " + JOB, "completed", "success"))])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("2 jobs", result.stderr)
        self.assertEqual(self.downloads(calls), [])

    def test_a_transient_listing_error_is_retried_within_the_bound(self):
        result, calls, received, _ = self.run_awaiter([
            "ERROR: HTTP 502 from the fixture\n",
            listing((JOB, "completed", "success")),
        ])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.downloads(calls)), 1)
        self.assertIn("decklock_0.0.0-1_arm64.deb", received)

    def test_listing_errors_that_persist_end_the_wait_long_before_its_bound(self):
        # Without actions: read every listing is refused; that must not idle a guest
        # for the whole wait.
        result, calls, _, elapsed = self.run_awaiter(
            ["ERROR: HTTP 403 from the fixture\n"], timeout="600",
            arguments=("--tolerated-errors", "3"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("HTTP 403 from the fixture", result.stderr)
        self.assertEqual(len([call for call in calls if call[0] == "api"]), 4)
        self.assertEqual(self.downloads(calls), [])
        self.assertLess(elapsed, 10)

    def test_a_missing_gh_is_reported_at_once(self):
        result, calls, _, elapsed = self.run_awaiter(
            [listing((JOB, "completed", "success"))], timeout="600", without_gh=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("gh", result.stderr)
        self.assertEqual(calls, [])
        self.assertLess(elapsed, 10)

    def test_checksums_are_verified_before_the_candidate_is_accepted(self):
        good = {"decklock_0.0.0-1_arm64.deb": b"candidate"}
        cases = {
            "mismatch": dict(files=good, sums=f"{'0' * 64}  decklock_0.0.0-1_arm64.deb\n"),
            "missing list": dict(files=good, sums=False),
            "empty list": dict(files=good, sums="\n"),
            "listed file absent": dict(
                files=good, sums=f"{hashlib.sha256(b'candidate').hexdigest()}  other.deb\n"),
            "path outside the artifact": dict(
                files=good, sums=f"{hashlib.sha256(b'candidate').hexdigest()}  ../dist/x.deb\n"),
        }
        for name, case in cases.items():
            with self.subTest(case=name):
                result, calls, _, _ = self.run_awaiter(
                    [listing((JOB, "completed", "success"))], **case)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(self.downloads(calls)), 1)
                self.assertNotIn("PASS", result.stdout)

    def test_a_destination_that_already_holds_files_is_refused(self):
        def stale(destination):
            destination.mkdir()
            (destination / "decklock_0.0.0-1_arm64.deb").write_bytes(b"left by something else")
        result, calls, received, _ = self.run_awaiter(
            [listing((JOB, "completed", "success"))], prepare=stale)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [], "A stale destination must be refused before any request")
        self.assertEqual(received["decklock_0.0.0-1_arm64.deb"], b"left by something else")

    def test_the_run_identity_is_required(self):
        for variable in ("GITHUB_REPOSITORY", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GH_TOKEN"):
            with self.subTest(variable=variable):
                result, calls, _, _ = self.run_awaiter(
                    [listing((JOB, "completed", "success"))], environment={variable: None})
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(variable, result.stderr)
                self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
