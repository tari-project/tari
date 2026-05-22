#!/usr/bin/env python3
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/integration_tests.yml"
FEATURES = ROOT / "integration_tests/tests/features"


def pr_ci_profile(workflow_text):
    matches = re.findall(r'echo "CI_PROFILE=([^"]+)" >> \$GITHUB_ENV', workflow_text)
    for value in matches:
        if "@critical" in value and "@long-running" in value:
            return value
    raise RuntimeError("PR CI_PROFILE was not found in integration_tests.yml")


def ffi_tag_expression(workflow_text):
    matches = re.findall(r'-- -t "([^"]*\@wallet-ffi[^"]*)"', workflow_text)
    for value in matches:
        if value.strip().startswith("(@wallet-ffi"):
            return value
    raise RuntimeError("FFI cucumber tag expression was not found in integration_tests.yml")


def tokenize(expression):
    return re.findall(r'\(|\)|\band\b|\bor\b|\bnot\b|@[A-Za-z0-9_-]+', expression)


class Parser:
    def __init__(self, tokens, tags):
        self.tokens = tokens
        self.tags = tags
        self.index = 0

    def parse(self):
        result = self.parse_or()
        if self.index != len(self.tokens):
            raise RuntimeError(f"unexpected token {self.tokens[self.index]}")
        return result

    def parse_or(self):
        result = self.parse_and()
        while self.peek() == "or":
            self.index += 1
            rhs = self.parse_and()
            result = result or rhs
        return result

    def parse_and(self):
        result = self.parse_not()
        while self.peek() == "and":
            self.index += 1
            rhs = self.parse_not()
            result = result and rhs
        return result

    def parse_not(self):
        if self.peek() == "not":
            self.index += 1
            return not self.parse_not()
        return self.parse_atom()

    def parse_atom(self):
        token = self.peek()
        if token == "(":
            self.index += 1
            result = self.parse_or()
            if self.peek() != ")":
                raise RuntimeError("missing closing parenthesis")
            self.index += 1
            return result
        if token and token.startswith("@"):
            self.index += 1
            return token in self.tags
        raise RuntimeError(f"unexpected token {token}")

    def peek(self):
        if self.index >= len(self.tokens):
            return None
        return self.tokens[self.index]


def evaluate(expression, tags):
    return Parser(tokenize(expression), tags).parse()


def scenarios():
    for feature in sorted(FEATURES.glob("*.feature")):
        feature_tags = []
        pending_tags = []
        saw_feature = False
        for line in feature.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if not stripped:
                continue
            if stripped.startswith("@"):
                pending_tags.extend(stripped.split())
                continue
            if stripped.startswith("Feature:"):
                feature_tags = pending_tags
                pending_tags = []
                saw_feature = True
                continue
            if stripped.startswith("Scenario:") or stripped.startswith("Scenario Outline:"):
                yield feature.relative_to(ROOT), stripped, set(feature_tags + pending_tags)
                pending_tags = []
                continue
            if saw_feature and pending_tags and not stripped.startswith("#"):
                pending_tags = []


def main():
    workflow_text = WORKFLOW.read_text(encoding="utf-8")
    expression = ffi_tag_expression(workflow_text).replace("${{ env.CI_PROFILE }}", pr_ci_profile(workflow_text))
    selected = [(path, name, tags) for path, name, tags in scenarios() if evaluate(expression, tags)]
    ffi_scenarios = [(path, name, tags) for path, name, tags in scenarios() if "@wallet-ffi" in tags or "@chat-ffi" in tags]
    print(f"PR FFI tag expression: {expression}")
    print(f"FFI scenarios in feature files: {len(ffi_scenarios)}")
    print(f"Selected PR FFI scenarios: {len(selected)}")
    if selected:
        for path, name, tags in selected:
            print(f"  {path}: {name} {' '.join(sorted(tags))}")
    if len(selected) == 0:
        print("FAIL: PR CI selects zero FFI scenarios")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
