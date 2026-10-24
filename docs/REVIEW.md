Русский перевод: [Перенос и ревью](ru/REVIEW.md).

# Migration and review

Date: 2026-10-24. The migration moves the requirements and plan from
`dvino/ak820` commit `e8ba2bcb352cd5c35e9cff4c645f89a463484e10` into
the separate `dvino/ak-configurator` repository.

## Source and scope

- Entropy base: `ergohaven/entropy` commit
  `5b222b5d6349fb351f53b36c66169095b3039fc1`. Git history, LICENSE and
  third-party notices are retained.
- Firmware reference: `dvino/ak820` commit
  `5cbc66fbaf22bf5372e76e892207f58d453d9183`.
- All 42 requirements keep their IDs. The documents now select Entropy,
  start native work first, keep browser development parallel and place
  configurator branches in the new repository.
- README, installation guidance and Status distinguish imported Entropy
  source from future AK820 adaptation. The BUILD introduction points to
  the new instructions. Rust, Cargo manifests, lockfiles, assets and
  workflows are unchanged from the pinned upstream base.
- Firmware documentation links to the new project. No configurator tree
  is added to the firmware development branch.

## Publication and branch cleanup

The new repository is prepared locally before publication. Independent review
and local documentation checks must finish before commits or pushes.
Configurator `main`, `desktop`, `desktop-dev` and `web-dev` start from the
same reviewed Entropy-based source; firmware stays separate.

The obsolete `web-dev`, `desktop` and `desktop-dev` refs in `dvino/ak820`
point to `e8ba2bcb352cd5c35e9cff4c645f89a463484e10` at migration start.
A complete local Git bundle of those refs was created and verified before
cleanup. Delete only those refs, after publishing the new repository, and
refuse deletion if a remote ref has advanced. Other firmware branches and
worktrees are outside this cleanup.

## Verification boundary

This package changes project documentation and repository placement only.
No application build, startup, device command, firmware write or hardware
test is part of this migration. Source inspection does not prove browser
permissions, OEM loader access, flash success or background memory use.

The imported release workflow still restricts publication to
`ergohaven/entropy`; adapting AK Configurator CI is planned work, not a
completed release gate.

## Completed checks and independent review

- Local link and language checks pass for 15 Markdown documents, including
  the migrated requirements, plan, Status, installation guidance and agent
  instructions. All 42 requirement IDs match the old EN source and both
  new language versions; there are no duplicates or unresolved placeholders.
- The firmware repository's `python3 tools/check_docs.py` passes for 49
  public documents. Whitespace checks pass in both migration diffs.
- Comparison with the pinned Entropy base shows no changes to runtime
  source, Cargo metadata, lockfiles, assets, workflows or LICENSE.
- Independent review covered both migration diffs, translations, source
  references, retained attribution, native/browser ordering, branch roles,
  release ownership and safety/privacy requirements. No actionable findings
  remained. This was a migration review, not a full audit of inherited code.
- Publication and remote-ref deletion occur only after this review. Their
  actual outcome is verified against remote refs, not inferred from the plan.
