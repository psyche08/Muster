import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "worktree-gc.sh"
ENV = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*args, cwd):
    return subprocess.run(
        ["git", *args], cwd=cwd, env=ENV, check=True, capture_output=True, text=True
    ).stdout


def repository(root):
    """A checkout on main with an `origin` holding the same commit."""
    origin = root / "origin.git"
    work = root / "work"
    subprocess.run(["git", "init", "-q", "--bare", str(origin)], check=True, env=ENV)
    subprocess.run(["git", "init", "-q", "-b", "main", str(work)], check=True, env=ENV)
    git("config", "user.email", "developer@example.invalid", cwd=work)
    git("config", "user.name", "developer", cwd=work)
    (work / "file.txt").write_text("one\n")
    git("add", "file.txt", cwd=work)
    git("commit", "-qm", "one", cwd=work)
    git("remote", "add", "origin", str(origin), cwd=work)
    git("push", "-q", "origin", "main", cwd=work)
    git("fetch", "-q", "origin", cwd=work)
    return work


class WorktreeGcTests(unittest.TestCase):
    def sweep(self, work, *args):
        return subprocess.run(
            ["bash", str(SCRIPT), "--dry-run", *args],
            cwd=work, env=ENV, capture_output=True, text=True,
        )

    def test_a_registered_path_without_its_git_dir_is_kept_not_fatal(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = repository(Path(tmp).resolve())
            done = Path(tmp).resolve() / "done"
            stale = Path(tmp).resolve() / "stale"
            git("worktree", "add", "-q", "-b", "done", str(done), "main", cwd=work)
            git("worktree", "add", "-q", "-b", "stale", str(stale), "main", cwd=work)
            # What is left when a worktree's administrative file goes but its
            # directory and its registration stay.
            (stale / ".git").unlink()

            result = self.sweep(work, "--min-idle", "0")

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn(f"kept     {stale}  (registered, but not a readable worktree)", result.stdout)
            # The sweep reaches the worktree after it instead of ending there.
            self.assertIn(f"would remove  {done}  (done, landed)", result.stdout)
            self.assertIn("1 worktrees would be removed", result.stdout)

    def test_an_unreadable_path_is_never_taken_for_a_clean_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = repository(Path(tmp).resolve())
            stale = Path(tmp).resolve() / "stale"
            git("worktree", "add", "-q", "-b", "stale", str(stale), "main", cwd=work)
            (stale / ".git").unlink()
            (stale / "file.txt").write_text("work nobody can see\n")

            result = self.sweep(work, "--min-idle", "0")

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn(f"would remove  {stale}", result.stdout)
            self.assertIn("0 worktrees would be removed", result.stdout)


if __name__ == "__main__":
    unittest.main()
