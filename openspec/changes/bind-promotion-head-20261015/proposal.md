# Bind immutable promotion candidates to PR heads

Parent: Conxian/conxian-business#1415; sprint: Conxian/conxian-business#1390.

Generated dev-to-staged and staged-to-main candidates must bind their branch suffix, recorded source, commit-window end and actual PR head to the same full lowercase commit SHA. Missing, malformed or changed heads must reject the candidate. A changed source requires a new candidate branch and fresh acceptance evidence.

Preserve ordinary work into dev, existing downward reconciliation paths, repository-specific policy, required owner review and real release acceptance. The trusted enforcement workflow continues to execute default-branch code with read-only permissions. Development tests are not evidence that the live default-branch policy has been promoted.

Acceptance: positive and negative cases for both directions, absent/malformed head evidence, stale source/window/base metadata, moving heads, forks, process exit codes and existing routes. Add path-scoped read-only CI for these tests. Runtime, signing, licensing and cloud configuration are outside this change.
