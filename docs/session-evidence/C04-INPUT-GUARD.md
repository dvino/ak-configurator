# C04 input guard — bounded MCP JSON framing

Russian: [RU](../ru/session-evidence/C04-INPUT-GUARD.md).

## State

**Preparatory transport guard only; the limited documentation/evidence review closed on 2026-11-03 with no remaining findings.** The existing RMCP stdio transport is wrapped by `StrictInput` before parsing. The guard rejects duplicate decoded JSON keys before values are consumed, malformed frames, invalid UTF-8 and lines larger than the inclusive 65,536-byte limit. Complete, in-limit JSON frames accepted by the guard are forwarded byte-for-byte, including CRLF and supported floating-point values. Empty, malformed, oversized and EOF-terminated partial frames are discarded; after an oversized frame it resynchronizes at the next LF, following provider behavior. No synthetic error reply is added.

**Historical checkpoint, 2026-11-03:** This report records the preparatory guard review before B11 acceptance and full C04 implementation. At that point the prepare tool was unimplemented and the MCP inventory had seven tools. Current implementation and qualification are recorded in [C04 evidence](C04.md); this guard report does not replace that current status.

Input arrives in 4,096-byte chunks and each poll processes at most 32 processing steps before yielding cooperatively. The line cap is not a 64 KiB peak-memory guarantee: parsing and allocator overhead add memory. No exact peak-RAM bound was established.

## Evidence

The focused `strict_input` unit run passed four tests (2020 filtered, 0.01 s) before the test module was moved to satisfy strict lint; the move changed test placement only, not runtime code. Repeated strict LINT-C passed after that move. Static `strict_input_review` reported no blocker. No independent reruns have occurred.

The STDIO duplicate-key check observed no unexpected response before a following `tools/list` exchange. This establishes that bounded scenario only; it does not prove that arbitrary delayed future replies can never occur. Source inspection and unit tests complement that subprocess observation.

## Scope boundary

At that historical checkpoint, this was transport input validation, not the C04 plan tool. `ak820_prepare`, plan digest/expiry/bindings and preparation/admission tests were not implemented, the inventory remained seven tools, and B11 acceptance was pending. These statements describe the earlier checkpoint; see current [C04 evidence](C04.md). No device or hardware calls occurred in the guard work.

**Terminal result:** the limited preparatory guard documentation/evidence scope is closed with no remaining findings. This result does not accept C04.
