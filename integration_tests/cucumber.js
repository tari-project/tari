module.exports = {
  default: "--tags 'not @long-running and not @broken' --fail-fast",
  ci: " ",
  critical: "--format @cucumber/pretty-formatter --tags @critical",
};
