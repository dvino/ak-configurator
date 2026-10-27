Русский перевод: [Маршрутизация агентов](ru/AGENT-ROUTING.md).

# Agent routing

| Role | Model | Reasoning | Scope |
| --- | --- | --- | --- |
| Lead | Current root agent | Current session | State, ownership, integration, plan and evidence |
| Routine worker | Fresh GPT-6 Luna | medium | Small edits, docs and routine builds |
| Implementer / visual worker | Fresh GPT-6.1 Terra (`gpt-6.1-terra`) | low | Bounded code implementation/refactoring, native UI and visual/browser/desktop tasks |
| Specialist | Fresh GPT-6.1 Sol | low | Complex ABI, NVM and technical contracts |
| Independent reviewer | Fresh GPT-6.1 Sol | low | Resulting code, docs and verification evidence |
| Architect | Fresh GPT-6.1 Sol | high | Last-resort architecture and complex planning |

The lead may adjust effort and routing within these role limits. Use the architect
only when ordinary implementation/review cannot settle a material architecture
or planning question; record the question and reason before delegation. Terra is
an implementation option, not a replacement for independent review.

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
