Русский перевод: [Маршрутизация агентов](ru/AGENT-ROUTING.md).

# Agent routing

Adapted from the AK820 firmware rules at `bef20f8`, 2026-10-25.

| Role | Model | Reasoning | Scope |
| --- | --- | --- | --- |
| Lead | Current root agent | Current session | Verify state, split ownership, integrate, update plan and evidence |
| Implementer | Fresh GPT-6 Luna | medium | Bounded implementation, documentation or build task |
| Specialist | Fresh GPT-6.1 Sol | low | Complex ABI, NVM and technical contract decisions |
| Architect (exceptional escalation) | Fresh GPT-6.1 Sol | high | Architecture and complex planning requiring deeper analysis |
| Independent reviewer | Fresh GPT-6.1 Sol | low | Exact resulting code, documentation and verification evidence |

Use the architect only as a last resort when implementation and ordinary review
cannot resolve a material architecture or complex-planning question. Record the unresolved question
and why deeper analysis is necessary before delegating. Routine implementation,
documentation, builds and normal review stay on the routes above.

The lead may adjust routing within these limits when the task requires it. Communicate with agents
in English and with the user in their chosen language. Start fresh contexts
for new bounded tasks; routine tasks receive a short self-contained handoff,
not the complete conversation.

Before delegation, check ownership and current dirty files. State owned files,
verified baseline, required evidence, dependencies and stop conditions. Avoid
duplicated work and simultaneous builds that share output paths. Authors cannot
approve their own changes. Resolve findings, retest corrections and request a
fresh independent review before commit or push.

Update the implementation plan while working. Keep source checks, builds,
offline models, platform launches and physical USB behavior separate. No agent
may send device setters, reset, loader transitions or flash commands without
explicit authorization for that operation. Never include OEM data, macros,
personal layouts, credentials or private paths in public diagnostic reports.
