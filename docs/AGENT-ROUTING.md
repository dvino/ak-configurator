Русский перевод: [Маршрутизация агентов](ru/AGENT-ROUTING.md).

# Agent routing

| Role | Model | Reasoning | Scope |
| --- | --- | --- | --- |
| Lead | Current root agent | Current session | State, ownership, integration, plan and evidence |
| Routine worker | Fresh GPT-6 Luna | medium | Small edits, simple implementation/refactoring, documentation, routine builds and browser/desktop visual work |
| Implementer | Fresh GPT-6.1 Terra (`gpt-6.1-terra`) | low | Complex implementation/refactoring and native UI work |
| Contract specialist / independent reviewer | Fresh GPT-6.1 Terra (`gpt-6.1-terra`) | medium | Complex ABI, NVM and technical contracts; independent review of resulting code, docs and evidence |
| Architect | Fresh GPT-6 Sol (`gpt-6-sol`) | low | Last-resort architecture and complex planning |

The lead may adjust effort and routing within these role limits. Use the architect
only when ordinary implementation and review cannot settle a material architecture
or planning question. Record the unresolved question and why the ordinary routes
cannot settle it before delegation. Do not route routine or simple tasks to the
architect. Authors cannot independently review their own work; Terra's review role
requires a fresh context separate from implementation.

Use English between agents and the user's language with the user. Start fresh
contexts for new bounded tasks; give concise self-contained handoffs with owned
files, verified baseline, required evidence, dependencies and stop conditions.
Check ownership/dirty work first; avoid duplicate work and shared build outputs.

Keep the plan current. Authors cannot approve their own changes. Resolve findings,
retest substantive corrections and obtain fresh independent review before commit
or push. Separate source checks, builds, offline models, platform launches and
physical USB behavior. Device setters, reset, loader and flash require existing
operation-specific authorization; this routing adds no approval step. Keep OEM
data, macros, personal layouts, credentials and private paths out of public reports.

For any long-running operation, including CI, use one persistent watcher script or
a native wait tied to the exact process, container or CI run. Do not manually poll logs/status repeatedly or start duplicate operations; only the
persistent watcher may poll the exact run as needed. Keep monitoring quiet unless the
operation completes, fails or needs action.
