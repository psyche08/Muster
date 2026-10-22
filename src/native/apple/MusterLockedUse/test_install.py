import unittest
from install import screensaver, RULE

class PolicyTests(unittest.TestCase):
    def test_default_and_existing_or_preserve_password(self):
        for rules in [["use-login-window-ui"], ["another.branch", "use-login-window-ui"]]:
            source = {"class": "rule", "rule": rules, "k-of-n": 1, "shared": True}
            result = screensaver(source)
            self.assertEqual(result["rule"], [RULE] + rules)
            self.assertEqual(source["rule"], rules)
            self.assertEqual(screensaver(result), result)
    def test_does_not_weaken_an_unknown_policy(self):
        for policy in [
            {"class": "rule", "rule": ["use-login-window-ui"], "k-of-n": 2},
            {"class": "rule", "rule": ["unknown", "use-login-window-ui"]},
            {"class": "allow"},
            {"class": "rule", "rule": ["use-login-window-ui"], "k-of-n": True},
            {"class": "evaluate-mechanisms", "mechanisms": ["builtin:authenticate"]},
        ]:
            with self.assertRaises(ValueError): screensaver(policy)

if __name__ == '__main__': unittest.main()
