# AGENTS.md

## Goal
Fix distributed filesystem bugs with minimal, targeted changes and verify them using xfstests.

Use an eval-driven workflow:
- reproduce
- analyze
- implement a focused fix
- validate
- iterate

## Repository validation rule
Always run xfstests from the repository root.

Primary validation command format:

```bash
bash docker/compose-xfstests/run_redis_xfstests.sh --cases "<explicit case list>"
IMPORTANT: Case selection rule

When testing multiple cases, you MUST list every case explicitly inside the --cases argument.

Correct example:

bash docker/compose-xfstests/run_redis_xfstests.sh --cases "generic/001 generic/002 generic/003 generic/004 generic/005 generic/006 generic/007 generic/008 generic/009 generic/010"

Do NOT use ranges such as:

generic/001...generic/010
generic/{001..010}
generic/001-010

The case list must always be written as a space-separated explicit enumeration.

Default batch size

Unless the task says otherwise, validate in batches of 10 cases.

Recommended progression example:

batch 1:
generic/001 generic/002 generic/003 generic/004 generic/005 generic/006 generic/007 generic/008 generic/009 generic/010
batch 2:
generic/011 generic/012 generic/013 generic/014 generic/015 generic/016 generic/017 generic/018 generic/019 generic/020
Test artifacts

After every validation run, inspect:

docker/compose-xfstests/artifacts/

Always use the newest artifacts/logs to determine:

which cases passed
which cases failed
what the dominant error signals are
Iteration rules

For each repair loop:

Run the current explicit 10-case batch.
Read the newest logs in docker/compose-xfstests/artifacts/.
Form the most likely root-cause hypothesis.
Make one focused code change.
Re-run the exact same explicit case list.
Compare before/after results.
Continue until the batch passes or the blocker is clearly identified.
Change policy

Do:

fix root causes
prefer minimal, localized patches
preserve filesystem semantics
keep explanations tied to observed test failures

Do not:

modify xfstests or the validation script unless absolutely necessary
skip, mute, or bypass failing tests to get green results
perform unrelated refactors
mix several speculative fixes into one iteration
