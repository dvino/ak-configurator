# Preserved work: initial comparison, 2026-10-28

Baseline: `desktop-dev` 4208f4c. The separate checkout at 193c16b and its uncommitted files are preserved. This is a limited source comparison, not acceptance or proof of equivalence of its five commits.

- The old executor's device-import undo is tied to the session, source, manifest and safety backup (`hid_ak820_executor.rs` at 193c16b). Current offline-project import undo (`ui/ak820_offline_files.rs`) is a different operation. Device-import undo remains to be assessed against current session and restore contracts.
- The old restore planner has separate macro and keymap reset scopes for alpha.3. Current `ak820_restore_plan.rs` and `ui/ak820_restore_request.rs` implement an integrated reset of settings with verified setters; inactive modules, reserved bytes and values without setters are retained. This does not establish equivalent selective reset. Do not transplant alpha.3 defaults to newer images without exact-image and scope evidence.
- The untracked release policy bundle is empty and the module reports information-only knowledge; mutable cache is rejected. Neither file is registered in the current build. Review authority, update and identity contracts before integration.
- The untracked schema migration and preview modules are not registered in the current build. Their synthetic fixture checksum passes, but its generator pins firmware commit `bef20f89d3da01d770767919bc7c8fc3c3a1a808`. This does not establish current transition admission, archive compatibility or device support. Current restore planning explicitly blocks schema01 without its semantic gate. The separate preview window also needs alignment with the planned unified interface.
- The four modified instruction/routing files differ from current upstream and would remove current rules if copied wholesale. Preserve and compare individual intentions; do not overwrite them.

Next: finish file-by-file comparison of all five commits and preserved modules; classify each difference and verify the exact current contracts before adapting a bounded change. No code, fixture regeneration, device operation or hardware validation was performed in this comparison.
