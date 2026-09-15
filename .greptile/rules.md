# Code Review Policy

## Genuine Issues Only

Report only genuine, actionable issues.

A finding must be supported by the actual repository code, architecture,
tests, or documented contracts.

Do NOT report:

- hypothetical edge cases
- speculative bugs
- extremely unlikely scenarios
- theoretical failure modes without a realistic execution path
- stylistic preferences
- subjective refactoring suggestions
- naming or formatting preferences
- unnecessary defensive programming
- "this could be improved" suggestions without concrete impact
- architecture changes without a demonstrated problem
- issues outside the scope of the current change

## Evidence Requirement

Before reporting an issue:

1. Inspect the relevant implementation.
2. Inspect surrounding code and call sites.
3. Inspect relevant tests.
4. Verify that the reported behavior is actually possible.
5. Identify a realistic execution path that produces the problem.
6. Determine the concrete impact.

If you cannot establish a realistic failure scenario, do not report it.

## False Positive Policy

When deciding between:

- a genuine issue, and
- a hypothetical or speculative concern

choose NOT to report the concern.

Prefer missing a low-confidence finding over reporting a false positive.

## Existing Architecture

Respect existing repository architecture and established contracts.

Do not recommend redesigning working code merely because another approach
could theoretically be cleaner.

Only recommend architectural changes when the current implementation causes
a concrete correctness, security, integrity, concurrency, reliability, or
maintainability problem.

## Finding Format

For every finding, include:

- Exact code/location
- Realistic failure scenario
- Concrete impact
- Why this is a genuine issue
- Practical fix direction

Severity should reflect the actual impact, not theoretical worst-case impact.